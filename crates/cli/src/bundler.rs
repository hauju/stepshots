use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use manifest::BundleManifest;
use zip::ZipWriter;
use zip::write::FileOptions;

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
