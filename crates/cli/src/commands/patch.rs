use std::collections::HashMap;
use std::path::Path;

use manifest::{BundleManifest, BundleManifestStep};

use crate::browser::Browser;
use crate::bundler::{create_bundle, read_bundle};
use crate::commands::record::{get_current_url, resolve_url};
use crate::error::CliError;

/// Amend a `.stepshot` bundle with manually captured steps. Opens a visible
/// browser at the bundle's stored viewport so captures are pixel-dimension
/// identical to the original recording — for flows that can't be re-recorded.
pub async fn run(
    bundle_path: &Path,
    replace: Option<usize>,
    at: Option<usize>,
    url: Option<&str>,
    profile_dir: Option<&Path>,
) -> Result<(), CliError> {
    let (mut manifest, screenshots, frames, dom_extracts) = read_bundle(bundle_path)?;
    let mode = resolve_mode(manifest.steps.len(), replace, at)?;
    let mut steps = std::mem::take(&mut manifest.steps);

    // Patching invalidates DOM extracts: `replace` swaps the screenshot out from
    // under one, and append/insert shift every later index. A step whose extract
    // and screenshot disagree would silently poison sandbox generation, so the
    // whole set is dropped and must be re-recorded.
    if !dom_extracts.is_empty() {
        println!(
            "Note: dropping {} DOM extract(s) — patching invalidates them. Re-record with --dom to regenerate.",
            dom_extracts.len()
        );
    }
    for step in &mut steps {
        step.dom = None;
    }

    let mut records = to_records(steps, screenshots, frames);

    let name = bundle_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| bundle_path.display().to_string());
    let vp = manifest.viewport.clone();
    let dsf = vp.device_scale_factor.unwrap_or(1.0);
    println!(
        "Patching {name} ({} steps, {}x{}{})",
        records.len(),
        vp.width,
        vp.height,
        if dsf == 1.0 {
            String::new()
        } else {
            format!(" @{dsf}x")
        }
    );
    match mode {
        Mode::Append => println!("Mode: append — new steps go after step {}", records.len()),
        Mode::Insert(n) => println!("Mode: insert at step {} (later steps shift back)", n + 1),
        Mode::Replace(n) => println!(
            "Mode: replace step {}'s screenshot (metadata and overlays are kept)",
            n + 1
        ),
    }

    let browser = Browser::launch(&vp, false, profile_dir).await?;
    if let Some(u) = start_url(mode, &records, &manifest, url) {
        println!("Opening browser at {u}");
        if let Err(e) = browser.navigate(&u).await {
            eprintln!("⚠ Could not open {u}: {e}");
        }
    }

    println!();
    println!("Stage the page exactly as the step should look (menus open, hover");
    println!("states set), then press Enter to capture. Press Ctrl+C when done to save.");
    println!();

    let mut seen = browser.page_ids().await?;
    let mut captures: Vec<(Vec<u8>, Option<String>)> = Vec::new();

    loop {
        let Some(line) = read_line_or_ctrl_c().await else {
            break;
        };
        if !line.trim().is_empty() {
            println!("Press Enter to capture, Ctrl+C to save and exit.");
            continue;
        }

        // Follow the user into any tab their clicks opened since last capture.
        let _ = browser.adopt_new_page(&seen).await;
        if let Ok(ids) = browser.page_ids().await {
            seen = ids;
        }

        match browser.screenshot().await {
            Ok(bytes) => {
                let page_url = get_current_url(&browser).await;
                match mode {
                    Mode::Replace(n) => {
                        captures.clear();
                        captures.push((bytes, page_url));
                        println!(
                            "  ✓ Captured replacement for step {} — Enter retakes, Ctrl+C saves",
                            n + 1
                        );
                    }
                    Mode::Insert(n) => {
                        captures.push((bytes, page_url));
                        println!("  ✓ Captured — will be step {}", n + captures.len());
                    }
                    Mode::Append => {
                        captures.push((bytes, page_url));
                        let at_url = captures
                            .last()
                            .and_then(|(_, u)| u.as_deref())
                            .map(|u| format!(" — {u}"))
                            .unwrap_or_default();
                        println!(
                            "  ✓ Captured step {}{at_url}",
                            records.len() + captures.len()
                        );
                    }
                }
            }
            Err(e) => eprintln!("  ✗ Capture failed: {e} — is the tab still open?"),
        }
    }

    if captures.is_empty() {
        println!("No captures — {name} unchanged.");
        return Ok(());
    }

    let kept = records.len();
    let summary;
    let mut replace_note = None;
    match mode {
        Mode::Replace(n) => {
            let (bytes, _) = captures.remove(0);
            records[n].screenshot = bytes;
            if has_editor_content(&records[n]) {
                replace_note = Some(format!(
                    "Note: kept step {}'s overlays — check their alignment in the dashboard editor.",
                    n + 1
                ));
            }
            summary = format!("{kept} steps: 1 replaced");
        }
        Mode::Append | Mode::Insert(_) => {
            let insert_at = match mode {
                Mode::Insert(n) => n,
                _ => records.len(),
            };
            let added = captures.len();
            let base_url = manifest.base_url.clone();
            for (offset, (bytes, page_url)) in captures.into_iter().enumerate() {
                records.insert(
                    insert_at + offset,
                    StepRecord {
                        step: new_manual_step(page_url, base_url.as_deref()),
                        screenshot: bytes,
                        transitions: None,
                    },
                );
            }
            summary = format!("{} steps: {kept} kept, {added} added", records.len());
        }
    }

    let (steps, shots, frames) = reassemble(records);
    manifest.steps = steps;
    let bak = save(bundle_path, &manifest, &shots, &frames)?;

    println!("✓ Saved {name} ({summary})");
    println!("  Backup of the original: {}", bak.display());
    if let Some(note) = replace_note {
        println!("  {note}");
    }
    println!(
        "Next: stepshots upload {} --demo-id <id>   # replace the hosted demo",
        bundle_path.display()
    );
    Ok(())
}

/// Patch mode with 0-based step indices.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Mode {
    Append,
    Insert(usize),
    Replace(usize),
}

/// Validate the 1-based `--replace` / `--at` flags against the step count.
fn resolve_mode(len: usize, replace: Option<usize>, at: Option<usize>) -> Result<Mode, CliError> {
    match (replace, at) {
        (Some(n), _) => {
            if n == 0 || n > len {
                return Err(CliError::Other(format!(
                    "--replace {n} is out of range: bundle has {len} steps (valid: 1-{len})"
                )));
            }
            Ok(Mode::Replace(n - 1))
        }
        (None, Some(n)) => {
            if n == 0 || n > len + 1 {
                return Err(CliError::Other(format!(
                    "--at {n} is out of range: bundle has {len} steps (valid: 1-{}, or omit --at to append)",
                    len + 1
                )));
            }
            Ok(Mode::Insert(n - 1))
        }
        (None, None) => Ok(Mode::Append),
    }
}

/// One step with its image bytes, so insert/replace/append is a single Vec op.
struct StepRecord {
    step: BundleManifestStep,
    screenshot: Vec<u8>,
    transitions: Option<Vec<Vec<u8>>>,
}

fn to_records(
    steps: Vec<BundleManifestStep>,
    screenshots: Vec<Vec<u8>>,
    mut frames: HashMap<usize, Vec<Vec<u8>>>,
) -> Vec<StepRecord> {
    steps
        .into_iter()
        .zip(screenshots)
        .enumerate()
        .map(|(i, (step, screenshot))| StepRecord {
            step,
            screenshot,
            transitions: frames.remove(&i),
        })
        .collect()
}

type Reassembled = (
    Vec<BundleManifestStep>,
    Vec<Vec<u8>>,
    HashMap<usize, Vec<Vec<u8>>>,
);

/// Regenerate positional `file` / transition paths and rebuild the shape
/// `create_bundle` takes.
fn reassemble(records: Vec<StepRecord>) -> Reassembled {
    let mut steps = Vec::with_capacity(records.len());
    let mut screenshots = Vec::with_capacity(records.len());
    let mut frames = HashMap::new();
    for (i, mut record) in records.into_iter().enumerate() {
        record.step.file = format!("steps/{i}.webp");
        record.step.transition_frames = record.transitions.as_ref().map(|f| {
            (0..f.len())
                .map(|j| format!("transitions/{i}/{j}.webp"))
                .collect()
        });
        if let Some(f) = record.transitions {
            frames.insert(i, f);
        }
        steps.push(record.step);
        screenshots.push(record.screenshot);
    }
    (steps, screenshots, frames)
}

/// A manually captured step: just the page URL — no action, selector, or
/// overlays. Hotspots can be added in the dashboard editor.
fn new_manual_step(url: Option<String>, base_url: Option<&str>) -> BundleManifestStep {
    let current_path = match (url.as_deref(), base_url) {
        (Some(u), Some(base)) => u.strip_prefix(base.trim_end_matches('/')).map(|path| {
            if path.is_empty() {
                "/".to_string()
            } else {
                path.to_string()
            }
        }),
        _ => None,
    };
    BundleManifestStep {
        file: String::new(),
        name: None,
        action: None,
        url,
        current_path,
        target_url: None,
        selector: None,
        selector_quality: None,
        target_text: None,
        target_aria: None,
        highlights: None,
        blur_regions: None,
        arrows: None,
        hotspots: None,
        popups: None,
        zoom_regions: None,
        text: None,
        key: None,
        scroll_x: None,
        scroll_y: None,
        scene_scroll_x: None,
        scene_scroll_y: None,
        value: None,
        delay: None,
        transition_frames: None,
        // Manually captured: there is no recording pass to extract from.
        dom: None,
    }
}

/// Where to navigate on launch: `--url` wins, then the mode's reference step
/// (replace: the step itself; insert: the step before the insertion point;
/// append: the last step), then the bundle's base URL + start path.
fn start_url(
    mode: Mode,
    records: &[StepRecord],
    manifest: &BundleManifest,
    url_flag: Option<&str>,
) -> Option<String> {
    if let Some(u) = url_flag {
        return Some(u.to_string());
    }
    let step_url = match mode {
        Mode::Replace(n) => records.get(n),
        Mode::Insert(n) => n.checked_sub(1).and_then(|p| records.get(p)),
        Mode::Append => records.last(),
    }
    .and_then(|r| r.step.url.clone());
    if step_url.is_some() {
        return step_url;
    }
    match (&manifest.base_url, &manifest.start_path) {
        (Some(base), Some(path)) => Some(resolve_url(base, path)),
        _ => None,
    }
}

fn has_editor_content(record: &StepRecord) -> bool {
    let s = &record.step;
    record.transitions.is_some()
        || s.highlights.is_some()
        || s.blur_regions.is_some()
        || s.arrows.is_some()
        || s.hotspots.is_some()
        || s.popups.is_some()
        || s.zoom_regions.is_some()
}

/// Write the new bundle to a temp file, back up the original, then atomically
/// rename over it. Returns the backup path.
fn save(
    bundle_path: &Path,
    manifest: &BundleManifest,
    screenshots: &[Vec<u8>],
    frames: &HashMap<usize, Vec<Vec<u8>>>,
) -> Result<std::path::PathBuf, CliError> {
    let tmp = bundle_path.with_extension("stepshot.tmp");
    let bak = bundle_path.with_extension("stepshot.bak");
    // No DOM extracts: `run` drops them, since patching invalidates them.
    create_bundle(manifest, screenshots, frames, &HashMap::new(), &tmp)?;
    std::fs::copy(bundle_path, &bak)?;
    std::fs::rename(&tmp, bundle_path)?;
    Ok(bak)
}

async fn read_line_or_ctrl_c() -> Option<String> {
    let line_future = tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        eprint!("patch> ");
        match std::io::stdin().read_line(&mut buf) {
            Ok(0) => None,
            Ok(_) => Some(buf),
            Err(_) => None,
        }
    });

    tokio::select! {
        result = line_future => {
            match result {
                Ok(Some(line)) => Some(line),
                _ => None,
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn step(value: serde_json::Value) -> BundleManifestStep {
        serde_json::from_value(value).unwrap()
    }

    fn fixture() -> Vec<StepRecord> {
        // Three steps; steps 2 and 3 carry transition frames.
        let steps = vec![
            step(json!({ "file": "steps/0.webp", "name": "one" })),
            step(json!({
                "file": "steps/1.webp",
                "name": "two",
                "transition_frames": ["transitions/1/0.webp"]
            })),
            step(json!({
                "file": "steps/2.webp",
                "name": "three",
                "transition_frames": ["transitions/2/0.webp", "transitions/2/1.webp"]
            })),
        ];
        let screenshots = vec![vec![0u8], vec![1u8], vec![2u8]];
        let mut frames = HashMap::new();
        frames.insert(1, vec![vec![10u8]]);
        frames.insert(2, vec![vec![20u8], vec![21u8]]);
        to_records(steps, screenshots, frames)
    }

    #[test]
    fn resolve_mode_bounds() {
        assert_eq!(resolve_mode(3, None, None).unwrap(), Mode::Append);
        assert_eq!(resolve_mode(3, None, Some(1)).unwrap(), Mode::Insert(0));
        // --at len+1 is a plain append via insert-at-end.
        assert_eq!(resolve_mode(3, None, Some(4)).unwrap(), Mode::Insert(3));
        assert!(resolve_mode(3, None, Some(0)).is_err());
        assert!(resolve_mode(3, None, Some(5)).is_err());
        assert_eq!(resolve_mode(3, Some(3), None).unwrap(), Mode::Replace(2));
        assert!(resolve_mode(3, Some(0), None).is_err());
        assert!(resolve_mode(3, Some(4), None).is_err());
        let msg = resolve_mode(3, None, Some(9)).unwrap_err().to_string();
        assert!(msg.contains("valid: 1-4"), "unexpected message: {msg}");
    }

    #[test]
    fn append_reindexes_files_and_frames() {
        let mut records = fixture();
        records.push(StepRecord {
            step: new_manual_step(Some("https://x.test/done".into()), Some("https://x.test")),
            screenshot: vec![3u8],
            transitions: None,
        });

        let (steps, shots, frames) = reassemble(records);
        assert_eq!(
            steps.iter().map(|s| s.file.as_str()).collect::<Vec<_>>(),
            [
                "steps/0.webp",
                "steps/1.webp",
                "steps/2.webp",
                "steps/3.webp"
            ]
        );
        assert_eq!(shots.len(), 4);
        assert_eq!(frames.get(&1).unwrap(), &vec![vec![10u8]]);
        assert_eq!(steps[3].current_path.as_deref(), Some("/done"));
    }

    #[test]
    fn insert_shifts_later_transition_frames() {
        let mut records = fixture();
        // Insert at step 2 (0-based index 1): old steps 2 and 3 shift back.
        records.insert(
            1,
            StepRecord {
                step: new_manual_step(None, None),
                screenshot: vec![9u8],
                transitions: None,
            },
        );

        let (steps, shots, frames) = reassemble(records);
        assert_eq!(steps[1].name, None);
        assert_eq!(steps[2].name.as_deref(), Some("two"));
        assert_eq!(shots[1], vec![9u8]);
        // Old step 2's frames moved from key 1 to key 2, step 3's from 2 to 3.
        assert!(!frames.contains_key(&1));
        assert_eq!(frames.get(&2).unwrap(), &vec![vec![10u8]]);
        assert_eq!(frames.get(&3).unwrap(), &vec![vec![20u8], vec![21u8]]);
        assert_eq!(
            steps[3].transition_frames.as_ref().unwrap(),
            &vec![
                "transitions/3/0.webp".to_string(),
                "transitions/3/1.webp".to_string()
            ]
        );
    }

    #[test]
    fn replace_keeps_metadata_and_swaps_bytes() {
        let mut records = fixture();
        records[1].screenshot = vec![99u8];

        let (steps, shots, frames) = reassemble(records);
        assert_eq!(steps[1].name.as_deref(), Some("two"));
        assert_eq!(shots[1], vec![99u8]);
        assert_eq!(frames.get(&1).unwrap(), &vec![vec![10u8]]);
    }

    #[test]
    fn manual_step_path_stripping() {
        let s = new_manual_step(
            Some("https://x.test/a/b".into()),
            Some("https://x.test/"), // trailing slash on base
        );
        assert_eq!(s.current_path.as_deref(), Some("/a/b"));

        let root = new_manual_step(Some("https://x.test".into()), Some("https://x.test"));
        assert_eq!(root.current_path.as_deref(), Some("/"));

        let no_base = new_manual_step(Some("https://x.test/a".into()), None);
        assert_eq!(no_base.current_path, None);

        let foreign = new_manual_step(Some("https://other.test/a".into()), Some("https://x.test"));
        assert_eq!(foreign.current_path, None);
    }
}
