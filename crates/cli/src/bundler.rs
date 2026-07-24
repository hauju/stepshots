use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use manifest::BundleManifest;
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::error::CliError;

/// Create a `.stepshot` bundle zip from a manifest, screenshot PNGs,
/// and optional transition JPEG frames keyed by step index.
pub fn create_bundle(
    manifest: &BundleManifest,
    screenshots: &[Vec<u8>],
    transition_frames: &HashMap<usize, Vec<Vec<u8>>>,
    output_path: &Path,
) -> Result<(), CliError> {
    if manifest.steps.len() != screenshots.len() {
        return Err(CliError::Bundle(format!(
            "Manifest has {} steps but got {} screenshots",
            manifest.steps.len(),
            screenshots.len()
        )));
    }

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(output_path)?;
    let mut zip = ZipWriter::new(file);

    let options =
        FileOptions::<'_, ()>::default().compression_method(zip::CompressionMethod::Deflated);

    // Write manifest.json
    let manifest_json = serde_json::to_string_pretty(manifest)?;
    zip.start_file("manifest.json", options)?;
    zip.write_all(manifest_json.as_bytes())?;

    // Write each screenshot
    for (i, png_bytes) in screenshots.iter().enumerate() {
        let filename = format!("steps/{i}.webp");
        zip.start_file(&filename, options)?;
        zip.write_all(png_bytes)?;
    }

    // Write transition frames
    for (step_idx, frames) in transition_frames {
        for (frame_idx, jpeg_bytes) in frames.iter().enumerate() {
            let filename = format!("transitions/{step_idx}/{frame_idx}.webp");
            zip.start_file(&filename, options)?;
            zip.write_all(jpeg_bytes)?;
        }
    }

    zip.finish()?;
    Ok(())
}

/// Manifest, per-step screenshot bytes, and transition frames keyed by step
/// index — the same shape `create_bundle` takes.
pub type BundleContents = (BundleManifest, Vec<Vec<u8>>, HashMap<usize, Vec<Vec<u8>>>);

/// Read a `.stepshot` bundle back into memory. Step images are resolved via
/// each step's `file` field (the server-side contract), not positionally.
/// Transition frames listed in the manifest but missing from the zip are
/// dropped with a warning instead of failing — a patched bundle may be the
/// user's only copy of an unrepeatable flow.
pub fn read_bundle(path: &Path) -> Result<BundleContents, CliError> {
    let bytes = std::fs::read(path)?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor)?;

    let manifest: BundleManifest = {
        let mut entry = archive.by_name("manifest.json").map_err(|_| {
            CliError::Bundle(format!("{}: no manifest.json in bundle", path.display()))
        })?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        serde_json::from_slice(&buf)?
    };

    let mut screenshots = Vec::with_capacity(manifest.steps.len());
    let mut transition_frames = HashMap::new();

    for (i, step) in manifest.steps.iter().enumerate() {
        screenshots.push(read_entry(&mut archive, &step.file).ok_or_else(|| {
            CliError::Bundle(format!(
                "step {} references missing file {} in {}",
                i + 1,
                step.file,
                path.display()
            ))
        })?);

        if let Some(frame_paths) = &step.transition_frames {
            let frames: Option<Vec<Vec<u8>>> = frame_paths
                .iter()
                .map(|p| read_entry(&mut archive, p))
                .collect();
            match frames {
                Some(frames) if !frames.is_empty() => {
                    transition_frames.insert(i, frames);
                }
                Some(_) => {}
                None => {
                    eprintln!(
                        "⚠ step {}: transition frames missing from bundle, dropping them",
                        i + 1
                    );
                }
            }
        }
    }

    Ok((manifest, screenshots, transition_frames))
}

/// Read one zip entry by manifest path, rejecting path traversal.
fn read_entry(archive: &mut ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> Option<Vec<u8>> {
    if name.contains("..") || name.starts_with('/') {
        return None;
    }
    let mut entry = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(steps: serde_json::Value) -> BundleManifest {
        serde_json::from_value(json!({
            "version": 1,
            "viewport": { "width": 800, "height": 600 },
            "steps": steps,
        }))
        .unwrap()
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "stepshots-bundler-test-{}-{name}",
            std::process::id()
        ))
    }

    #[test]
    fn round_trips_steps_and_transitions() {
        let manifest = manifest(json!([
            { "file": "steps/0.webp" },
            { "file": "steps/1.webp", "transition_frames": ["transitions/1/0.webp", "transitions/1/1.webp"] },
        ]));
        let screenshots = vec![vec![1u8, 2], vec![3u8, 4]];
        let mut frames = HashMap::new();
        frames.insert(1, vec![vec![5u8], vec![6u8]]);

        let path = temp_path("roundtrip.stepshot");
        create_bundle(&manifest, &screenshots, &frames, &path).unwrap();
        let (read_manifest, read_shots, read_frames) = read_bundle(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(read_manifest.steps.len(), 2);
        assert_eq!(read_shots, screenshots);
        assert_eq!(read_frames, frames);
    }

    #[test]
    fn drops_missing_transition_frames_without_error() {
        // Manifest lists a transition frame that create_bundle never writes
        // (no entry in the frames map) — read_bundle must warn and drop it.
        let manifest = manifest(json!([
            { "file": "steps/0.webp", "transition_frames": ["transitions/0/0.webp"] },
        ]));
        let screenshots = vec![vec![9u8]];

        let path = temp_path("tolerant.stepshot");
        create_bundle(&manifest, &screenshots, &HashMap::new(), &path).unwrap();
        let (_, read_shots, read_frames) = read_bundle(&path).unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(read_shots, screenshots);
        assert!(read_frames.is_empty());
    }

    #[test]
    fn errors_on_missing_step_image() {
        // Manifest points at a file create_bundle never writes (it writes
        // steps/0.webp positionally), so the read must hard-error.
        let manifest = manifest(json!([{ "file": "steps/nope.webp" }]));
        let path = temp_path("missing-step.stepshot");
        create_bundle(&manifest, &[vec![1u8]], &HashMap::new(), &path).unwrap();

        let err = read_bundle(&path).unwrap_err();
        std::fs::remove_file(&path).unwrap();
        assert!(err.to_string().contains("missing file"));
    }
}
