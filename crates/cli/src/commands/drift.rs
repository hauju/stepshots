//! `stepshots drift` — check recorded demos against the live app by diffing
//! DOM extracts, without replaying the flow.
//!
//! Complements `verify` (which replays a demo) and `tour check` (which replays
//! a tour). Replay catches broken *actions* but cascades — one hard failure
//! blinds every later step — and only ever inspects step targets. This loads
//! each distinct route once and diffs the whole page, so it never cascades and
//! sees changes no step points at. See `docs/drift-detection-spec.md`.
//!
//! Runs against the customer's own app with their own session: no server-side
//! capture, no credential custody, no model spend.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use manifest::{BundleManifestStep, DomExtract};

use crate::browser::Browser;
use crate::bundler::read_bundle;
use crate::drift::{Severity, StepDrift, Verdict, diff, unanchored_steps};
use crate::error::CliError;
use crate::output::{DriftAsset, DriftOutput, DriftRoute, DriftSummary, ErrorOutput};

/// What escalates to a non-zero exit. Mirrors `verify`'s `--fail-on` shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FailOn {
    /// Only a broken anchor (the demo can no longer play) fails the run.
    Stale,
    /// Any drift at all fails the run.
    Drifted,
}

/// One route to check: the path, plus the step whose extract is the baseline.
struct Route {
    path: String,
    step_index: usize,
    baseline: DomExtract,
    step: BundleManifestStep,
}

/// Group steps by `current_path`, keeping the first step that carries an
/// extract. Steps sharing a route show the same page, so checking each would
/// re-load the same URL N times for the same answer.
fn routes(
    steps: &[BundleManifestStep],
    extracts: &std::collections::HashMap<usize, Vec<u8>>,
) -> Result<Vec<Route>, CliError> {
    let mut seen: BTreeMap<String, Route> = BTreeMap::new();

    for (i, step) in steps.iter().enumerate() {
        let Some(bytes) = extracts.get(&i) else {
            continue;
        };
        let path = step
            .current_path
            .clone()
            .or_else(|| step.url.clone())
            .unwrap_or_else(|| "/".to_string());
        if seen.contains_key(&path) {
            continue;
        }
        let baseline: DomExtract = serde_json::from_slice(bytes).map_err(|e| {
            CliError::Bundle(format!("step {}: unreadable DOM extract: {e}", i + 1))
        })?;
        seen.insert(
            path.clone(),
            Route {
                path,
                step_index: i + 1,
                baseline,
                step: step.clone(),
            },
        );
    }

    Ok(seen.into_values().collect())
}

fn join_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

pub async fn run(
    bundles: &[PathBuf],
    url_override: Option<&str>,
    fail_on: FailOn,
    json: bool,
    session: crate::browser::SessionArgs<'_>,
) -> Result<(), CliError> {
    let storage_state = session.load(!json)?;
    let mut assets: Vec<DriftAsset> = Vec::new();
    let mut worst = Verdict::Ok;

    for path in bundles {
        let asset = check_bundle(
            path,
            url_override,
            json,
            session.with_state(storage_state.as_ref()),
        )
        .await?;
        worst = max_verdict(worst, verdict_of(&asset));
        assets.push(asset);
    }

    if json {
        let out = DriftOutput {
            success: !exits_nonzero(worst, fail_on),
            command: "drift",
            verdict: format!("{worst:?}").to_lowercase(),
            assets: Some(assets),
            error: None::<ErrorOutput>,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing DriftOutput")
        );
    }

    if exits_nonzero(worst, fail_on) {
        return Err(CliError::Reported { code: 1 });
    }
    Ok(())
}

async fn check_bundle(
    bundle_path: &Path,
    url_override: Option<&str>,
    json: bool,
    session: crate::browser::SessionSource<'_>,
) -> Result<DriftAsset, CliError> {
    let name = bundle_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| bundle_path.display().to_string());

    let (manifest, _screens, _frames, extracts) = read_bundle(bundle_path)?;

    if extracts.is_empty() {
        if !json {
            println!("\n{name}");
            println!("  ⚠ no DOM extracts — re-record with --dom to enable drift checks");
        }
        return Ok(DriftAsset {
            name,
            verdict: "unsupported".into(),
            routes: Vec::new(),
            unanchored: Vec::new(),
        });
    }

    let base = url_override
        .map(str::to_string)
        .or_else(|| manifest.base_url.clone())
        .ok_or_else(|| {
            CliError::Bundle(format!("{name}: no base URL in the bundle — pass --url"))
        })?;

    let to_check = routes(&manifest.steps, &extracts)?;
    let browser = Browser::launch_with_session(&manifest.viewport, true, session).await?;
    // A cached response would make a changed page look unchanged — a false
    // "no drift" on a demo that is actually broken.
    browser.disable_cache().await?;

    let mut results = Vec::new();
    for route in &to_check {
        let target = join_url(&base, &route.path);
        browser.navigate(&target).await?;
        browser.wait_idle(route.step.delay.unwrap_or(800)).await;

        // Redact the live capture exactly as the baseline was redacted. This is
        // required for correctness, not just privacy: a baseline with blurred
        // text diffed against an unredacted live page reports every blurred
        // node as a text change.
        let blurs = route.step.blur_regions.clone().unwrap_or_default();
        let opts = crate::dom_extract::DomExtractOpts {
            blur_regions: &blurs,
            ..Default::default()
        };

        let Some(live) = browser.extract_dom(&opts).await? else {
            return Err(CliError::Browser(format!(
                "{name}: could not extract DOM at {target}"
            )));
        };

        let d = diff(&route.baseline, &live, &manifest.steps);
        results.push((route.path.clone(), route.step_index, target, d));
    }

    let unanchored = unanchored_steps(&manifest.steps);

    if !json {
        print_asset(&name, &results, &unanchored);
    }

    let verdict = results
        .iter()
        .map(|(_, _, _, d)| d.verdict)
        .fold(Verdict::Ok, max_verdict);

    Ok(DriftAsset {
        name,
        verdict: format!("{verdict:?}").to_lowercase(),
        routes: results
            .into_iter()
            .map(|(path, step, url, d)| DriftRoute {
                path,
                url,
                first_step: step,
                verdict: format!("{:?}", d.verdict).to_lowercase(),
                summary: summary_of(&d),
                findings: d.findings,
            })
            .collect(),
        unanchored: unanchored
            .into_iter()
            .map(|(step, selector)| format!("step {step}: {selector}"))
            .collect(),
    })
}

fn summary_of(d: &StepDrift) -> DriftSummary {
    let (critical, content, layout) = d.counts();
    DriftSummary {
        critical,
        content,
        layout,
        nodes_before: d.nodes_before,
        nodes_after: d.nodes_after,
    }
}

fn verdict_of(asset: &DriftAsset) -> Verdict {
    match asset.verdict.as_str() {
        "stale" => Verdict::Stale,
        "drifted" => Verdict::Drifted,
        _ => Verdict::Ok,
    }
}

fn max_verdict(a: Verdict, b: Verdict) -> Verdict {
    use Verdict::*;
    match (a, b) {
        (Stale, _) | (_, Stale) => Stale,
        (Drifted, _) | (_, Drifted) => Drifted,
        _ => Ok,
    }
}

fn exits_nonzero(worst: Verdict, fail_on: FailOn) -> bool {
    match fail_on {
        FailOn::Stale => worst == Verdict::Stale,
        FailOn::Drifted => worst != Verdict::Ok,
    }
}

fn print_asset(
    name: &str,
    results: &[(String, usize, String, StepDrift)],
    unanchored: &[(usize, String)],
) {
    println!("\n{name}");

    for (path, step, _url, d) in results {
        let (critical, content, layout) = d.counts();
        let mark = match d.verdict {
            Verdict::Ok => "✓",
            Verdict::Drifted => "~",
            Verdict::Stale => "✗",
        };
        println!(
            "  {mark} {path}  (from step {step}) — {}",
            match d.verdict {
                Verdict::Ok => "no changes".to_string(),
                Verdict::Drifted => format!("{content} content, {layout} layout"),
                Verdict::Stale =>
                    format!("{critical} breaking, {content} content, {layout} layout"),
            }
        );

        for f in &d.findings {
            // Critical findings always print. Everything else is capped so a
            // wholesale redesign can't bury the one line that matters.
            let show = f.severity == Severity::Critical
                || d.findings
                    .iter()
                    .filter(|g| g.severity == f.severity)
                    .take(6)
                    .any(|g| std::ptr::eq(g, f));
            if !show {
                continue;
            }
            let tag = match f.severity {
                Severity::Critical => "!",
                Severity::Content => "-",
                Severity::Layout => "~",
            };
            match f.step {
                Some(n) => println!("      {tag} step {n} targets {} — {}", f.what, f.detail),
                None => println!("      {tag} {} — {}", f.what, f.detail),
            }
        }
    }

    if !unanchored.is_empty() {
        println!("  ⚠ steps with no durable anchor (selector-only, so undetectable drift):");
        for (step, selector) in unanchored {
            println!("      step {step}: {selector}");
        }
        println!("      add an aria-label to these targets, or re-record to capture one");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fail_on_stale_ignores_drift() {
        assert!(!exits_nonzero(Verdict::Drifted, FailOn::Stale));
        assert!(exits_nonzero(Verdict::Stale, FailOn::Stale));
        assert!(!exits_nonzero(Verdict::Ok, FailOn::Stale));
    }

    #[test]
    fn fail_on_drifted_catches_both() {
        assert!(exits_nonzero(Verdict::Drifted, FailOn::Drifted));
        assert!(exits_nonzero(Verdict::Stale, FailOn::Drifted));
        assert!(!exits_nonzero(Verdict::Ok, FailOn::Drifted));
    }

    #[test]
    fn worst_verdict_wins_across_routes() {
        assert_eq!(max_verdict(Verdict::Ok, Verdict::Drifted), Verdict::Drifted);
        assert_eq!(
            max_verdict(Verdict::Drifted, Verdict::Stale),
            Verdict::Stale
        );
        assert_eq!(max_verdict(Verdict::Ok, Verdict::Ok), Verdict::Ok);
    }

    #[test]
    fn absolute_step_urls_are_not_re_joined() {
        assert_eq!(join_url("https://a.test", "/x"), "https://a.test/x");
        assert_eq!(join_url("https://a.test/", "x"), "https://a.test/x");
        assert_eq!(
            join_url("https://a.test", "https://b.test/y"),
            "https://b.test/y"
        );
    }

    #[test]
    fn routes_dedupe_by_path_and_skip_steps_without_extracts() {
        let steps: Vec<BundleManifestStep> = serde_json::from_value(serde_json::json!([
            { "file": "steps/0.webp", "current_path": "/a" },
            { "file": "steps/1.webp", "current_path": "/a" },
            { "file": "steps/2.webp", "current_path": "/b" },
        ]))
        .unwrap();

        let extract = serde_json::json!({
            "v": 1,
            "viewport": { "width": 10, "height": 10 },
            "root": { "tag": "body", "b": [0.0, 0.0, 10.0, 10.0] }
        })
        .to_string()
        .into_bytes();

        let mut extracts = std::collections::HashMap::new();
        extracts.insert(0, extract.clone());
        extracts.insert(1, extract.clone());
        // step 2 (/b) deliberately has no extract

        let r = routes(&steps, &extracts).unwrap();
        assert_eq!(r.len(), 1, "/a once, /b skipped for lacking an extract");
        assert_eq!(r[0].path, "/a");
        assert_eq!(r[0].step_index, 1);
    }
}
