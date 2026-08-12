use std::io::Read;
use std::path::Path;

use manifest::BundleManifest;
use reqwest::multipart;

use crate::error::CliError;
use crate::output::{UploadOutput, UploadedDemo};

#[allow(dead_code)]
pub struct UploadResult {
    pub demo_id: String,
    pub view_url: String,
}

/// Upload one or more `.stepshot` bundles to the Stepshots API.
/// If `replace_demo_id` is set, replaces that existing demo instead of creating new ones.
/// `public` makes newly created demos publicly viewable at insert time.
/// `json` suppresses human-readable output and prints a single JSON object instead.
pub async fn run(
    files: &[String],
    title_override: Option<&str>,
    replace_demo_id: Option<&str>,
    public: bool,
    json: bool,
    server_url: &str,
    token: &str,
) -> Result<Vec<UploadResult>, CliError> {
    let client = reqwest::Client::new();
    let mut results = Vec::new();

    for file_path in files {
        results.push(
            upload_one(
                &client,
                file_path,
                title_override,
                replace_demo_id,
                public,
                json,
                server_url,
                token,
            )
            .await?,
        );
    }

    if json {
        let out = UploadOutput {
            success: true,
            command: "upload",
            demos: results
                .iter()
                .map(|r| UploadedDemo {
                    demo_id: r.demo_id.clone(),
                    view_url: r.view_url.clone(),
                    replaced: replace_demo_id.is_some(),
                })
                .collect(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing UploadOutput")
        );
    }

    Ok(results)
}

/// Upload or replace a single bundle. `quiet` suppresses all progress output —
/// the MCP server calls this with `quiet: true` because its stdout carries the
/// JSON-RPC stream.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn upload_one(
    client: &reqwest::Client,
    file_path: &str,
    title_override: Option<&str>,
    replace_demo_id: Option<&str>,
    public: bool,
    quiet: bool,
    server_url: &str,
    token: &str,
) -> Result<UploadResult, CliError> {
    let json = quiet;
    {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err(CliError::Upload(format!("File not found: {file_path}")));
        }

        let bundle_bytes = std::fs::read(path)?;
        let bundle_file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("bundle.stepshot")
            .to_string();

        if let Some(demo_id) = replace_demo_id {
            // Replace existing demo
            if !json {
                println!("Replacing demo {demo_id} with: {file_path}");
            }

            let url = format!(
                "{}/api/demos/{demo_id}/replace-bundle",
                server_url.trim_end_matches('/')
            );

            let resp = send_with_retry(
                || {
                    let form = multipart::Form::new().part(
                        "bundle",
                        multipart::Part::bytes(bundle_bytes.clone())
                            .file_name(bundle_file_name.clone())
                            .mime_str("application/zip")
                            .map_err(|e| CliError::Upload(format!("MIME error: {e}")))?,
                    );
                    Ok(client
                        .put(&url)
                        .header("Authorization", format!("Bearer {token}"))
                        .multipart(form))
                },
                json,
            )
            .await?;

            if resp.status().is_success() {
                let view_url = format!(
                    "{}/dashboard/demos/{demo_id}",
                    server_url.trim_end_matches('/')
                );
                if !json {
                    println!("  Replaced! Demo ID: {demo_id}");
                    println!("  View at: {view_url}");
                }
                Ok(UploadResult {
                    demo_id: demo_id.to_string(),
                    view_url,
                })
            } else {
                return Err(upload_error("Replace", resp).await);
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

            if !json {
                println!("Uploading: {file_path} as \"{title}\"");
            }

            let url = format!(
                "{}/api/demos/upload-bundle",
                server_url.trim_end_matches('/')
            );

            let resp = send_with_retry(
                || {
                    let mut form = multipart::Form::new().text("title", title.clone());
                    if public {
                        form = form.text("public", "true");
                    }
                    let form = form.part(
                        "bundle",
                        multipart::Part::bytes(bundle_bytes.clone())
                            .file_name(bundle_file_name.clone())
                            .mime_str("application/zip")
                            .map_err(|e| CliError::Upload(format!("MIME error: {e}")))?,
                    );
                    Ok(client
                        .post(&url)
                        .header("Authorization", format!("Bearer {token}"))
                        .multipart(form))
                },
                json,
            )
            .await?;

            if resp.status().is_success() {
                let body: serde_json::Value = resp.json().await?;
                let demo_id = body
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| CliError::Upload("API response missing 'id' field".into()))?
                    .to_string();
                let view_url = format!(
                    "{}/dashboard/demos/{demo_id}",
                    server_url.trim_end_matches('/')
                );
                if !json {
                    println!("  Uploaded! Demo ID: {demo_id}");
                    println!("  View at: {view_url}");
                    if !public {
                        println!(
                            "  New demos start private — publish and get embed code from that page."
                        );
                    }
                }
                Ok(UploadResult { demo_id, view_url })
            } else {
                return Err(upload_error("Upload", resp).await);
            }
        }
    }
}

/// Send an upload request with bounded retry: connection errors, timeouts and
/// 5xx responses back off exponentially (1s, 2s, 4s) before giving up; 4xx
/// fail fast. The builder closure runs once per attempt because a multipart
/// body can't be reused after a send.
async fn send_with_retry<F>(build: F, json: bool) -> Result<reqwest::Response, CliError>
where
    F: Fn() -> Result<reqwest::RequestBuilder, CliError>,
{
    const MAX_ATTEMPTS: u32 = 4;
    let mut attempt = 1;
    loop {
        let result = build()?.send().await;
        let retryable = match &result {
            Ok(resp) => resp.status().is_server_error(),
            Err(e) => e.is_connect() || e.is_timeout(),
        };
        if !retryable || attempt >= MAX_ATTEMPTS {
            return Ok(result?);
        }
        let delay = std::time::Duration::from_secs(1 << (attempt - 1));
        if !json {
            match &result {
                Ok(resp) => println!(
                    "  Server error ({}), retrying in {}s…",
                    resp.status(),
                    delay.as_secs()
                ),
                Err(e) => println!(
                    "  Connection error ({e}), retrying in {}s…",
                    delay.as_secs()
                ),
            }
        }
        tokio::time::sleep(delay).await;
        attempt += 1;
    }
}

/// Turn a non-success upload response into a helpful error. A 401 means the
/// token is bad, so return an `Auth` error with a re-login hint instead of a
/// bare status; everything else keeps the server's error message.
async fn upload_error(action: &str, resp: reqwest::Response) -> CliError {
    let status = resp.status();

    if status.as_u16() == 401 {
        return CliError::Auth(
            "API token is invalid or expired. Run `stepshots login` again, \
             or check --token / STEPSHOTS_TOKEN."
                .into(),
        );
    }

    let body = resp.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(String::from))
        .unwrap_or(body);
    CliError::Upload(format!("{action} failed ({status}): {message}"))
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
