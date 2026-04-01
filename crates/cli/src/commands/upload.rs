use std::io::Read;
use std::path::Path;

use manifest::BundleManifest;
use reqwest::multipart;

use crate::error::CliError;

#[allow(dead_code)]
pub struct UploadResult {
    pub demo_id: String,
    pub view_url: String,
}

/// Upload one or more `.stepshot` bundles to the Stepshots API.
/// If `replace_demo_id` is set, replaces that existing demo instead of creating new ones.
pub async fn run(
    files: &[String],
    title_override: Option<&str>,
    replace_demo_id: Option<&str>,
    server_url: &str,
    token: &str,
) -> Result<Vec<UploadResult>, CliError> {
    let client = reqwest::Client::new();
    let mut results = Vec::new();

    for file_path in files {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(CliError::Upload(format!("File not found: {file_path}")));
        }

        let bundle_bytes = std::fs::read(path)?;

        if let Some(demo_id) = replace_demo_id {
            // Replace existing demo
            println!("Replacing demo {demo_id} with: {file_path}");

            let form = multipart::Form::new().part(
                "bundle",
                multipart::Part::bytes(bundle_bytes)
                    .file_name(
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("bundle.stepshot")
                            .to_string(),
                    )
                    .mime_str("application/zip")
                    .map_err(|e| CliError::Upload(format!("MIME error: {e}")))?,
            );

            let url = format!(
                "{}/api/demos/{demo_id}/replace-bundle",
                server_url.trim_end_matches('/')
            );

            let resp = client
                .put(&url)
                .header("Authorization", format!("Bearer {token}"))
                .multipart(form)
                .send()
                .await?;

            if resp.status().is_success() {
                let view_url = format!("{}/demos/{demo_id}", server_url.trim_end_matches('/'));
                println!("  Replaced! Demo ID: {demo_id}");
                println!("  View at: {view_url}");
                results.push(UploadResult {
                    demo_id: demo_id.to_string(),
                    view_url,
                });
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                    .unwrap_or(body);
                return Err(CliError::Upload(format!(
                    "Replace failed ({status}): {message}"
                )));
            }
        } else {
            // Create new demo
            let title = if let Some(t) = title_override {
                t.to_string()
            } else {
                extract_title_from_bundle(&bundle_bytes).unwrap_or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("Untitled")
                        .to_string()
                })
            };

            println!("Uploading: {file_path} as \"{title}\"");

            let form = multipart::Form::new().text("title", title.clone()).part(
                "bundle",
                multipart::Part::bytes(bundle_bytes)
                    .file_name(
                        path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("bundle.stepshot")
                            .to_string(),
                    )
                    .mime_str("application/zip")
                    .map_err(|e| CliError::Upload(format!("MIME error: {e}")))?,
            );

            let url = format!(
                "{}/api/demos/upload-bundle",
                server_url.trim_end_matches('/')
            );

            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {token}"))
                .multipart(form)
                .send()
                .await?;

            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await?;
                let demo_id = body
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let view_url = format!("{}/demos/{demo_id}", server_url.trim_end_matches('/'));
                println!("  Uploaded! Demo ID: {demo_id}");
                println!("  View at: {view_url}");
                results.push(UploadResult { demo_id, view_url });
            } else {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                let message = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
                    .unwrap_or(body);
                return Err(CliError::Upload(format!(
                    "Upload failed ({status}): {message}"
                )));
            }
        }
    }

    Ok(results)
}

/// Try to extract a title from the bundle's manifest.json.
fn extract_title_from_bundle(bundle_bytes: &[u8]) -> Option<String> {
    let cursor = std::io::Cursor::new(bundle_bytes);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut manifest_file = archive.by_name("manifest.json").ok()?;
    let mut buf = Vec::new();
    manifest_file.read_to_end(&mut buf).ok()?;
    let manifest: BundleManifest = serde_json::from_slice(&buf).ok()?;
    // BundleManifest doesn't have a title field, so return None
    let _ = manifest;
    None
}
