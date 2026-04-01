use std::io::Read;
use std::path::Path;

use manifest::BundleManifest;

use crate::error::CliError;

/// Read and parse the manifest from a `.stepshot` bundle ZIP.
pub fn read_bundle_manifest(path: &Path) -> Result<BundleManifest, CliError> {
    let file = std::fs::File::open(path)
        .map_err(|e| CliError::Bundle(format!("Failed to open bundle {}: {e}", path.display())))?;

    let mut archive = zip::ZipArchive::new(file)?;

    let mut manifest_entry = archive
        .by_name("manifest.json")
        .map_err(|e| CliError::Bundle(format!("No manifest.json in bundle: {e}")))?;

    let mut contents = String::new();
    manifest_entry.read_to_string(&mut contents)?;

    let manifest: BundleManifest = serde_json::from_str(&contents)?;
    Ok(manifest)
}
