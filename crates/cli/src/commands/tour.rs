//! `stepshots tour export` — project a recorded `.stepshot` bundle into a live
//! guided-tour track (JSON) for the `@stepshots/tour` player.
//!
//! A recorded flow already carries everything a live tour needs: each step's
//! `selector`, `action`, `name`, and highlight `callout`. This projects that
//! onto the player's track schema so one recording drives both a screenshot demo
//! AND a live "light the way" walkthrough.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::ValueEnum;
use manifest::BundleManifest;
use serde::Serialize;

use crate::error::CliError;

/// Output format for `tour export`.
#[derive(Copy, Clone, ValueEnum)]
pub enum TourFormat {
    /// A `{ "<key>": { "steps": [...] } }` registry (fetch, import, or inline).
    Json,
    /// A script that assigns `window.__STEPSHOTS_TOURS` (drop-in `<script src>`).
    Js,
}

#[derive(Serialize)]
struct Advance {
    #[serde(rename = "type")]
    kind: &'static str,
}

#[derive(Serialize)]
struct TourStep {
    selector: String,
    title: String,
    body: String,
    advance: Advance,
}

#[derive(Serialize)]
struct TourTrack {
    steps: Vec<TourStep>,
}

/// Map a recorded action to how the live step advances. Only interactive actions
/// become tour steps; `navigate`/`wait`/etc. are setup and are dropped.
fn action_to_advance(action: &str) -> Option<&'static str> {
    match action {
        "click" => Some("click"),
        "type" => Some("input"),
        _ => None,
    }
}

fn read_manifest(bundle: &Path) -> Result<BundleManifest, CliError> {
    let bytes = std::fs::read(bundle)?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor)?;
    let mut entry = archive.by_name("manifest.json").map_err(|_| {
        CliError::Bundle(format!("{}: no manifest.json in bundle", bundle.display()))
    })?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

/// Project the bundle onto the tour-track schema. A step is included only if it
/// has a highlight callout AND an interactive action; setup steps are dropped —
/// the same convention that lets one recording serve both demo and tour.
fn project(manifest: &BundleManifest) -> TourTrack {
    let steps = manifest
        .steps
        .iter()
        .filter_map(|s| {
            let body = s.highlights.as_ref()?.first()?.callout.clone()?;
            let kind = action_to_advance(s.action.as_deref()?)?;
            let selector = s.selector.clone()?;
            Some(TourStep {
                selector,
                title: s.name.clone().unwrap_or_default(),
                body,
                advance: Advance { kind },
            })
        })
        .collect();
    TourTrack { steps }
}

/// Export a tour track from `bundle` to `output` (default `<bundle>.tour.json`),
/// registered under `key` (default the bundle's filename stem). The output is the
/// `{ "<key>": { "steps": [...] } }` registry shape `window.__STEPSHOTS_TOURS` expects.
pub fn export(
    bundle: &Path,
    output: Option<PathBuf>,
    key: Option<String>,
    format: TourFormat,
    json: bool,
) -> Result<(), CliError> {
    let stem = bundle
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("tour")
        .to_string();
    let key = key.unwrap_or(stem);
    let default_ext = match format {
        TourFormat::Json => "tour.json",
        TourFormat::Js => "tour.js",
    };
    let out_path = output.unwrap_or_else(|| bundle.with_extension(default_ext));

    let manifest = read_manifest(bundle)?;
    let track = project(&manifest);

    if track.steps.is_empty() {
        return Err(CliError::Bundle(format!(
            "{}: no tour steps derived — recorded steps need a highlight callout + a click/type action",
            bundle.display()
        )));
    }

    let step_count = track.steps.len();
    let mut registry: BTreeMap<String, TourTrack> = BTreeMap::new();
    registry.insert(key.clone(), track);
    let registry_json = serde_json::to_string_pretty(&registry)?;
    // JS format merges into any existing registry so multiple exports can coexist.
    let contents = match format {
        TourFormat::Json => format!("{registry_json}\n"),
        TourFormat::Js => format!(
            "window.__STEPSHOTS_TOURS = Object.assign(window.__STEPSHOTS_TOURS || {{}}, {registry_json});\n"
        ),
    };
    std::fs::write(&out_path, contents)?;

    if json {
        let out = serde_json::json!({
            "success": true,
            "command": "tour export",
            "bundle": bundle.display().to_string(),
            "output": out_path.display().to_string(),
            "key": key,
            "steps": step_count,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour export output")
        );
    } else {
        println!("Exported tour \"{key}\" ({step_count} steps) → {}", out_path.display());
        for track in registry.values() {
            for s in &track.steps {
                println!("  - {:<5} {}  \"{}\"", s.advance.kind, s.selector, s.title);
            }
        }
    }

    Ok(())
}
