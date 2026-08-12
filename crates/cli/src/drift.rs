//! Drift detection — diff a recorded DOM extract against the live app.
//!
//! Complements the replay checks (`verify`, `tour check`) rather than replacing
//! them. Replay walks the flow and catches broken *actions*, but it cascades —
//! a hard failure at step 2 means steps 3..N are never evaluated — and it only
//! ever looks at step targets. This looks at the whole page in one pass and
//! never cascades, but is blind to behaviour. See `docs/drift-detection-spec.md`.
//!
//! Everything here is a pure function over [`DomExtract`], so the noise
//! behaviour is testable without a browser. That matters more than usual: the
//! spike went from 246 findings to 3 for the same five changes purely by fixing
//! how findings are grouped and ranked, and a regression there would make the
//! feature useless without making any test fail.

use std::collections::{BTreeMap, HashMap, HashSet};

use manifest::{BundleManifestStep, DomExtract, DomNode};
use serde::Serialize;

/// Movement below this many pixels is sub-pixel/antialiasing noise, not drift.
const MOVE_TOLERANCE_PX: f64 = 4.0;
/// Own text longer than this is treated as content rather than identity — long
/// paragraphs are edited routinely and make poor anchors.
const MAX_ANCHOR_TEXT: usize = 60;
/// Layout groups smaller than this are reported individually instead of as a
/// group, since "1 node shifted" reads worse than naming the node.
const MIN_GROUP: usize = 2;

/// How severe a finding is, ranked against what the recording actually uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A node the recording anchors to is gone or renamed — playback or tour
    /// projection will break.
    Critical,
    /// The page shows something the product no longer does, outside the anchor
    /// set. Playback still works; the demo is misleading.
    Content,
    /// Geometry moved. Recorded overlay coordinates are wrong.
    Layout,
}

/// Overall state of one asset, sharing `verify`/`tour check` vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Ok,
    Drifted,
    Stale,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub severity: Severity,
    /// Machine-readable kind: "removed", "added", "text", "moved", "unanchored".
    pub kind: &'static str,
    /// The anchor this concerns, e.g. `aria:Blur` or `txt:Preview`.
    pub what: String,
    /// Human-readable specifics.
    pub detail: String,
    /// 1-based step index, when the finding is tied to a specific step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<usize>,
    /// Nodes covered — >1 only for collapsed layout groups.
    #[serde(skip_serializing_if = "is_one")]
    pub nodes: usize,
}

fn is_one(n: &usize) -> bool {
    *n == 1
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepDrift {
    pub verdict: Verdict,
    pub findings: Vec<Finding>,
    pub nodes_before: usize,
    pub nodes_after: usize,
}

impl StepDrift {
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for f in &self.findings {
            match f.severity {
                Severity::Critical => c.0 += 1,
                Severity::Content => c.1 += 1,
                Severity::Layout => c.2 += 1,
            }
        }
        c
    }
}

/// An element's identity, most durable first. Deliberately the same ladder the
/// tour player and `tour check` use to re-resolve a drifted target — a node this
/// can't re-anchor is a node the player would also fail to find.
///
/// Redacted nodes anchor structurally instead. Their text is `█` blocks sized to
/// the original string, so a value that merely *changed length* — `$1,284` to
/// `$12,847` — would otherwise produce a different key and read as one element
/// removed and another added. Every live dashboard would show permanent drift.
fn anchor_of(node: &DomNode, path: &str) -> String {
    if node.redacted == Some(true) {
        return format!("path:{path}");
    }
    if let Some(aria) = &node.aria {
        return format!("aria:{aria}");
    }
    match &node.txt {
        Some(txt) if txt.chars().count() < MAX_ANCHOR_TEXT => format!("txt:{txt}"),
        _ => format!("path:{path}"),
    }
}

/// Structural signature: tag plus its first two classes. Survives the usual
/// class churn (added utilities, state classes) while still distinguishing
/// siblings of different kinds.
fn signature(node: &DomNode) -> String {
    let classes: Vec<&str> = node
        .cls
        .as_deref()
        .unwrap_or_default()
        .split_whitespace()
        .take(2)
        .collect();
    if classes.is_empty() {
        node.tag.clone()
    } else {
        format!("{}.{}", node.tag, classes.join("."))
    }
}

/// Flatten to anchor -> node, disambiguating repeats by occurrence order so
/// three buttons labelled "1" stay distinguishable across a diff.
fn index(extract: &DomExtract) -> HashMap<String, DomNode> {
    let mut out = HashMap::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut stack = vec![(&extract.root, String::new())];

    while let Some((node, parent)) = stack.pop() {
        let path = format!("{parent}/{}", signature(node));
        let base = anchor_of(node, &path);
        let n = seen.entry(base.clone()).or_insert(0);
        *n += 1;
        let key = if *n == 1 { base } else { format!("{base}#{n}") };
        out.insert(key, node.clone());
        for kid in &node.kids {
            stack.push((kid, path.clone()));
        }
    }
    out
}

/// Strip the `#N` occurrence suffix to get back the bare anchor.
fn base_of(key: &str) -> &str {
    key.split('#').next().unwrap_or(key)
}

/// Anchors this recording depends on: the text/aria identity captured for each
/// element-targeting step. These are the only nodes whose disappearance
/// provably breaks playback.
fn manifest_anchors(steps: &[BundleManifestStep]) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for (i, step) in steps.iter().enumerate() {
        if let Some(aria) = &step.target_aria {
            out.insert(format!("aria:{aria}"), i + 1);
        }
        if let Some(text) = &step.target_text {
            out.insert(format!("txt:{text}"), i + 1);
        }
    }
    out
}

/// Steps that target an element but captured no durable identity — icon-only
/// buttons with no `aria-label`, typically. Neither this nor the tour player's
/// fallback path can re-anchor them, so they're worth surfacing at check time
/// rather than after a break.
pub fn unanchored_steps(steps: &[BundleManifestStep]) -> Vec<(usize, String)> {
    steps
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.selector.is_some()
                && s.target_aria.is_none()
                && s.target_text.is_none()
                && matches!(
                    s.action.as_deref(),
                    Some("click" | "type" | "select" | "hover")
                )
        })
        .map(|(i, s)| (i + 1, s.selector.clone().unwrap_or_default()))
        .collect()
}

/// How far a replacement candidate may sit from the element it replaces.
/// A renamed button stays roughly where it was; something across the page is a
/// different control that happens to share a tag.
const HEAL_RADIUS_PX: f64 = 120.0;

/// Find the most likely replacement for an element that disappeared.
///
/// Scoped deliberately: same tag, appeared since the recording, and near where
/// the original sat. Anything looser starts proposing plausible-but-wrong
/// anchors, and a heal that silently re-points a demo at the wrong element is
/// worse than a reported break — the demo keeps running and quietly lies.
///
/// Returns `None` rather than guessing when two candidates are comparably close;
/// an ambiguous suggestion is a decision the human should make.
fn suggest_replacement(gone: &DomNode, added: &[(&String, &DomNode)]) -> Option<(String, f64)> {
    let centre = |n: &DomNode| (n.b[0] + n.b[2] / 2.0, n.b[1] + n.b[3] / 2.0);
    let (gx, gy) = centre(gone);

    let mut scored: Vec<(&String, f64)> = added
        .iter()
        .filter(|(_, n)| n.tag == gone.tag)
        .map(|(k, n)| {
            let (cx, cy) = centre(n);
            (*k, ((cx - gx).powi(2) + (cy - gy).powi(2)).sqrt())
        })
        .filter(|(_, d)| *d <= HEAL_RADIUS_PX)
        .collect();

    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    match scored.as_slice() {
        [] => None,
        // A clear winner needs to be meaningfully closer than the runner-up.
        [(k, d)] => Some(((*k).clone(), *d)),
        [(k, d), (_, d2), ..] if *d2 - *d > 24.0 => Some(((*k).clone(), *d)),
        _ => None,
    }
}

/// Whether a text/aria anchor still exists anywhere on the captured page.
///
/// Used for tours, which carry no baseline extract to diff against — only the
/// fallback anchors the player would use. Answering "is this anchor still
/// present" per step, from one capture, gives `tour check`'s verdict without
/// its replay: a step that can't be reached no longer hides every step after it.
pub fn has_anchor(extract: &DomExtract, aria: Option<&str>, text: Option<&str>) -> bool {
    let mut found = false;
    extract.visit(&mut |node| {
        if found {
            return;
        }
        if let (Some(want), Some(got)) = (aria, node.aria.as_deref())
            && want == got
        {
            found = true;
        }
        if let (Some(want), Some(got)) = (text, node.txt.as_deref())
            && want.trim() == got.trim()
        {
            found = true;
        }
    });
    found
}

/// Quantised bounds delta, used as the cascade-collapse grouping key.
fn delta(before: &DomNode, after: &DomNode) -> Option<[i64; 4]> {
    let d: Vec<f64> = before
        .b
        .iter()
        .zip(after.b.iter())
        .map(|(x, y)| y - x)
        .collect();
    if d.iter().all(|v| v.abs() <= MOVE_TOLERANCE_PX) {
        return None;
    }
    // Round to whole pixels so a 24.0 and a 24.04 shift group together.
    Some([
        d[0].round() as i64,
        d[1].round() as i64,
        d[2].round() as i64,
        d[3].round() as i64,
    ])
}

fn describe_delta(d: [i64; 4]) -> String {
    let mut parts = Vec::new();
    if d[0] != 0 {
        parts.push(format!(
            "{}px {}",
            d[0].abs(),
            if d[0] < 0 { "left" } else { "right" }
        ));
    }
    if d[1] != 0 {
        parts.push(format!(
            "{}px {}",
            d[1].abs(),
            if d[1] < 0 { "up" } else { "down" }
        ));
    }
    if d[2] != 0 {
        parts.push(format!("width {:+}px", d[2]));
    }
    if d[3] != 0 {
        parts.push(format!("height {:+}px", d[3]));
    }
    if parts.is_empty() {
        "resized".to_string()
    } else {
        parts.join(", ")
    }
}

/// Diff one step's recorded extract against the live page.
pub fn diff(before: &DomExtract, after: &DomExtract, steps: &[BundleManifestStep]) -> StepDrift {
    let a = index(before);
    let b = index(after);
    let anchors = manifest_anchors(steps);
    let mut findings = Vec::new();

    let a_keys: HashSet<&String> = a.keys().collect();
    let b_keys: HashSet<&String> = b.keys().collect();

    // Elements that appeared since the recording — the candidate pool a
    // vanished anchor could have been renamed into.
    let added_nodes: Vec<(&String, &DomNode)> =
        b_keys.difference(&a_keys).map(|k| (*k, &b[*k])).collect();

    // --- removed ---------------------------------------------------------
    for key in a_keys.difference(&b_keys) {
        let node = &a[*key];
        let base = base_of(key);
        if let Some(step) = anchors.get(base) {
            let detail = match suggest_replacement(node, &added_nodes) {
                Some((candidate, dist)) => format!(
                    "targeted by this step, absent from the live page — likely renamed to {} ({dist:.0}px away)",
                    base_of(&candidate)
                ),
                None => "targeted by this step, absent from the live page".into(),
            };
            findings.push(Finding {
                severity: Severity::Critical,
                kind: "removed",
                what: base.to_string(),
                detail,
                step: Some(*step),
                nodes: 1,
            });
        } else if node.aria.is_some() || node.tag == "button" || node.tag == "a" {
            findings.push(Finding {
                severity: Severity::Content,
                kind: "removed",
                what: base.to_string(),
                detail: "interactive element present at record time, gone now".into(),
                step: None,
                nodes: 1,
            });
        }
        // Non-interactive removals are usually cascade debris — a container
        // that vanished takes its children with it. Reporting each one is the
        // noise that made the first spike unusable.
    }

    // --- added -----------------------------------------------------------
    for key in b_keys.difference(&a_keys) {
        let node = &b[*key];
        if node.aria.is_some() || node.tag == "button" || node.tag == "a" {
            findings.push(Finding {
                severity: Severity::Content,
                kind: "added",
                what: base_of(key).to_string(),
                detail: "interactive element added since recording".into(),
                step: None,
                nodes: 1,
            });
        }
    }

    // --- changed + moved -------------------------------------------------
    // Moves are accumulated by delta and collapsed afterwards: one container
    // resize shifts hundreds of descendants by an identical amount, and each
    // is a symptom of the same single change.
    let mut groups: BTreeMap<[i64; 4], Vec<String>> = BTreeMap::new();

    for key in a_keys.intersection(&b_keys) {
        let (before_node, after_node) = (&a[*key], &b[*key]);
        let base = base_of(key);

        // A redacted node's text is block characters, and the region was blurred
        // precisely because it holds volatile data. Comparing it reports the
        // data changing — which is expected, not drift.
        let redacted = before_node.redacted == Some(true) || after_node.redacted == Some(true);

        if let (Some(old), Some(new)) = (&before_node.txt, &after_node.txt)
            && old != new
            && !redacted
        {
            let step = anchors.get(base).copied();
            findings.push(Finding {
                severity: if step.is_some() {
                    Severity::Critical
                } else {
                    Severity::Content
                },
                kind: "text",
                what: base.to_string(),
                detail: format!("{old:?} -> {new:?}"),
                step,
                nodes: 1,
            });
        }

        if let Some(d) = delta(before_node, after_node) {
            groups.entry(d).or_default().push(base.to_string());
        }
    }

    for (d, mut keys) in groups {
        keys.sort();
        let detail = describe_delta(d);
        if keys.len() < MIN_GROUP {
            findings.push(Finding {
                severity: Severity::Layout,
                kind: "moved",
                what: keys.remove(0),
                detail,
                step: None,
                nodes: 1,
            });
        } else {
            let sample: Vec<String> = keys.iter().take(3).cloned().collect();
            findings.push(Finding {
                severity: Severity::Layout,
                kind: "moved",
                what: format!("{} nodes", keys.len()),
                detail: format!("{detail} (e.g. {})", sample.join(", ")),
                step: None,
                nodes: keys.len(),
            });
        }
    }

    findings.sort_by(|x, y| x.severity.cmp(&y.severity).then(x.kind.cmp(y.kind)));

    let verdict = if findings.iter().any(|f| f.severity == Severity::Critical) {
        Verdict::Stale
    } else if findings.is_empty() {
        Verdict::Ok
    } else {
        Verdict::Drifted
    };

    StepDrift {
        verdict,
        findings,
        nodes_before: a.len(),
        nodes_after: b.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use manifest::{DomTokens, Viewport};

    fn node(tag: &str, b: [f64; 4]) -> DomNode {
        DomNode {
            tag: tag.into(),
            cls: None,
            b,
            s: Default::default(),
            txt: None,
            role: None,
            aria: None,
            kind: None,
            asset: None,
            palette: vec![],
            redacted: None,
            kids: vec![],
        }
    }

    fn labelled(tag: &str, aria: &str, b: [f64; 4]) -> DomNode {
        let mut n = node(tag, b);
        n.aria = Some(aria.into());
        n
    }

    fn texted(tag: &str, txt: &str, b: [f64; 4]) -> DomNode {
        let mut n = node(tag, b);
        n.txt = Some(txt.into());
        n
    }

    fn extract(root: DomNode) -> DomExtract {
        DomExtract {
            v: 1,
            viewport: Viewport {
                width: 1440,
                height: 810,
                device_scale_factor: None,
            },
            url: None,
            truncated: false,
            stats: None,
            tokens: DomTokens::default(),
            root,
        }
    }

    fn step(aria: Option<&str>, text: Option<&str>) -> BundleManifestStep {
        serde_json::from_value(serde_json::json!({
            "file": "steps/0.webp",
            "action": "click",
            "selector": "button",
            "target_aria": aria,
            "target_text": text,
        }))
        .unwrap()
    }

    #[test]
    fn identical_extracts_are_ok() {
        let mut root = node("body", [0.0, 0.0, 100.0, 100.0]);
        root.kids = vec![labelled("button", "Save", [0.0, 0.0, 10.0, 10.0])];
        let d = diff(&extract(root.clone()), &extract(root), &[]);
        assert_eq!(d.verdict, Verdict::Ok);
        assert!(d.findings.is_empty());
    }

    #[test]
    fn missing_step_target_is_critical() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        before.kids = vec![labelled("button", "Blur", [0.0, 0.0, 10.0, 10.0])];
        let after = node("body", [0.0, 0.0, 100.0, 100.0]);

        let d = diff(
            &extract(before),
            &extract(after),
            &[step(Some("Blur"), None)],
        );
        assert_eq!(d.verdict, Verdict::Stale);
        let f = &d.findings[0];
        assert_eq!(f.severity, Severity::Critical);
        assert_eq!(f.step, Some(1));
        assert_eq!(f.what, "aria:Blur");
    }

    #[test]
    fn renaming_a_targeted_label_is_critical() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        before.kids = vec![texted("span", "Preview", [0.0, 0.0, 10.0, 10.0])];
        let mut after = node("body", [0.0, 0.0, 100.0, 100.0]);
        after.kids = vec![texted("span", "Viewer", [0.0, 0.0, 10.0, 10.0])];

        // Text is the identity, so a rename reads as removed+added — the
        // removal is what must be flagged against the step.
        let d = diff(
            &extract(before),
            &extract(after),
            &[step(None, Some("Preview"))],
        );
        assert_eq!(d.verdict, Verdict::Stale);
        assert!(
            d.findings
                .iter()
                .any(|f| f.severity == Severity::Critical && f.what == "txt:Preview")
        );
    }

    #[test]
    fn untargeted_change_is_drifted_not_stale() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        before.kids = vec![labelled("button", "Fit", [0.0, 0.0, 10.0, 10.0])];
        let after = node("body", [0.0, 0.0, 100.0, 100.0]);

        let d = diff(&extract(before), &extract(after), &[]);
        assert_eq!(d.verdict, Verdict::Drifted);
        assert_eq!(d.findings[0].severity, Severity::Content);
    }

    #[test]
    fn subpixel_movement_is_not_drift() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        before.kids = vec![labelled("button", "Save", [10.0, 10.0, 10.0, 10.0])];
        let mut after = node("body", [0.0, 0.0, 100.0, 100.0]);
        after.kids = vec![labelled("button", "Save", [12.5, 10.0, 10.0, 10.0])];

        let d = diff(&extract(before), &extract(after), &[]);
        assert_eq!(d.verdict, Verdict::Ok, "{:?}", d.findings);
    }

    /// The regression test that matters: one container resize shifting many
    /// descendants must collapse to a single finding. Without this the spike
    /// produced 246 findings for 5 real changes.
    #[test]
    fn cascading_moves_collapse_into_one_finding() {
        let shift = 24.0;
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        let mut after = node("body", [0.0, 0.0, 100.0, 100.0]);
        for i in 0..50 {
            let y = i as f64;
            before
                .kids
                .push(labelled("div", &format!("row{i}"), [100.0, y, 10.0, 10.0]));
            after.kids.push(labelled(
                "div",
                &format!("row{i}"),
                [100.0 - shift, y, 10.0, 10.0],
            ));
        }

        let d = diff(&extract(before), &extract(after), &[]);
        let moves: Vec<&Finding> = d.findings.iter().filter(|f| f.kind == "moved").collect();
        assert_eq!(moves.len(), 1, "50 identical shifts must collapse to one");
        assert_eq!(moves[0].nodes, 50);
        assert!(
            moves[0].detail.starts_with("24px left"),
            "{}",
            moves[0].detail
        );
    }

    #[test]
    fn distinct_deltas_stay_separate() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        let mut after = node("body", [0.0, 0.0, 100.0, 100.0]);
        for (i, dx) in [24.0_f64, 24.0, 12.0].iter().enumerate() {
            before
                .kids
                .push(labelled("div", &format!("r{i}"), [100.0, 0.0, 10.0, 10.0]));
            after.kids.push(labelled(
                "div",
                &format!("r{i}"),
                [100.0 - dx, 0.0, 10.0, 10.0],
            ));
        }
        let d = diff(&extract(before), &extract(after), &[]);
        let moves: Vec<&Finding> = d.findings.iter().filter(|f| f.kind == "moved").collect();
        // Two distinct deltas: one group of 2 collapsed, one lone node named.
        assert_eq!(moves.len(), 2);
        assert!(moves.iter().any(|m| m.nodes == 2));
        assert!(moves.iter().any(|m| m.nodes == 1));
    }

    /// Blurred regions hold volatile data by definition — a metric that merely
    /// changes value must not read as drift, or every live dashboard is
    /// permanently "stale".
    #[test]
    fn redacted_values_changing_length_are_not_drift() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        let mut old_cell = texted(
            "td",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            [0.0, 0.0, 40.0, 10.0],
        );
        old_cell.redacted = Some(true);
        before.kids = vec![old_cell];

        let mut after = node("body", [0.0, 0.0, 100.0, 100.0]);
        // One character longer: $1,284 -> $12,847
        let mut new_cell = texted(
            "td",
            "\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}",
            [0.0, 0.0, 40.0, 10.0],
        );
        new_cell.redacted = Some(true);
        after.kids = vec![new_cell];

        let d = diff(&extract(before), &extract(after), &[]);
        assert_eq!(d.verdict, Verdict::Ok, "{:?}", d.findings);
    }

    #[test]
    fn drift_around_a_redacted_node_is_still_caught() {
        let mut before = node("body", [0.0, 0.0, 100.0, 100.0]);
        let mut cell = texted("td", "\u{2588}\u{2588}\u{2588}", [0.0, 0.0, 40.0, 10.0]);
        cell.redacted = Some(true);
        before.kids = vec![cell, labelled("button", "Export", [50.0, 0.0, 10.0, 10.0])];

        let mut after = node("body", [0.0, 0.0, 100.0, 100.0]);
        let mut cell2 = texted(
            "td",
            "\u{2588}\u{2588}\u{2588}\u{2588}",
            [0.0, 0.0, 40.0, 10.0],
        );
        cell2.redacted = Some(true);
        after.kids = vec![cell2];

        let d = diff(&extract(before), &extract(after), &[]);
        assert!(
            d.findings.iter().any(|f| f.what == "aria:Export"),
            "redaction must not mask a real removal: {:?}",
            d.findings
        );
    }

    #[test]
    fn a_renamed_button_in_place_is_suggested() {
        let mut before = node("body", [0.0, 0.0, 200.0, 100.0]);
        before.kids = vec![texted("button", "Preview", [10.0, 10.0, 60.0, 20.0])];
        let mut after = node("body", [0.0, 0.0, 200.0, 100.0]);
        after.kids = vec![texted("button", "Viewer", [10.0, 10.0, 60.0, 20.0])];

        let d = diff(
            &extract(before),
            &extract(after),
            &[step(None, Some("Preview"))],
        );
        let crit = d
            .findings
            .iter()
            .find(|f| f.severity == Severity::Critical)
            .unwrap();
        assert!(
            crit.detail.contains("likely renamed to txt:Viewer"),
            "{}",
            crit.detail
        );
    }

    #[test]
    fn a_distant_element_is_not_suggested() {
        let mut before = node("body", [0.0, 0.0, 900.0, 900.0]);
        before.kids = vec![texted("button", "Preview", [10.0, 10.0, 60.0, 20.0])];
        let mut after = node("body", [0.0, 0.0, 900.0, 900.0]);
        // Same tag, but across the page — a different control, not a rename.
        after.kids = vec![texted("button", "Delete", [700.0, 800.0, 60.0, 20.0])];

        let d = diff(
            &extract(before),
            &extract(after),
            &[step(None, Some("Preview"))],
        );
        let crit = d
            .findings
            .iter()
            .find(|f| f.severity == Severity::Critical)
            .unwrap();
        assert!(!crit.detail.contains("likely renamed"), "{}", crit.detail);
    }

    /// Two equally-plausible candidates must produce no suggestion. Picking one
    /// would re-point the demo at a coin-flip.
    #[test]
    fn ambiguous_candidates_produce_no_suggestion() {
        let mut before = node("body", [0.0, 0.0, 200.0, 100.0]);
        before.kids = vec![texted("button", "Preview", [50.0, 10.0, 20.0, 20.0])];
        let mut after = node("body", [0.0, 0.0, 200.0, 100.0]);
        after.kids = vec![
            texted("button", "Alpha", [30.0, 10.0, 20.0, 20.0]),
            texted("button", "Beta", [70.0, 10.0, 20.0, 20.0]),
        ];

        let d = diff(
            &extract(before),
            &extract(after),
            &[step(None, Some("Preview"))],
        );
        let crit = d
            .findings
            .iter()
            .find(|f| f.severity == Severity::Critical)
            .unwrap();
        assert!(!crit.detail.contains("likely renamed"), "{}", crit.detail);
    }

    #[test]
    fn icon_only_steps_are_reported_as_unanchored() {
        let steps = vec![step(None, None), step(Some("Blur"), None)];
        let un = unanchored_steps(&steps);
        assert_eq!(un.len(), 1);
        assert_eq!(un[0].0, 1);
    }
}
