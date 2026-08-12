//! `stepshots sandbox` — generate and push AI-rebuilt interactive replicas.
//!
//! Generation runs **on the user's machine, on the user's own Claude account**
//! — their `ANTHROPIC_API_KEY`, or their local Claude Code agent when no key
//! is set (no key to manage; the subscription they already have covers it).
//! Either way the DOM extracts feeding the model never leave the machine, and
//! the output is a committable single-file HTML artifact — the same git-first
//! model as `.tour.json`. `push` uploads the reviewed artifact to the
//! dashboard, which re-validates it and hosts it.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use futures::StreamExt as _;
use manifest::{BundleManifest, validate_sandbox_html};

use crate::bundler::read_bundle;
use crate::error::CliError;

/// Hard ceiling on steps sent to the model. Bounds cost per generation; a
/// longer recording contributes its first N steps, preferring at least one
/// per route (see `select_steps`).
const MAX_PROMPT_STEPS: usize = 12;

/// The generator contract, stated as hard rules. Rules 1–5 mirror
/// `docs/dom-extract-spec.md` § Generator contract; the output-shape section
/// exists so `validate_sandbox_html` can enforce bounded surface mechanically.
const SYSTEM_PROMPT: &str = r#"You rebuild a customer's web application as a single self-contained, interactive HTML sandbox, working from screenshots, a recording manifest, and per-step DOM extracts (exact text, class attributes, bounds in viewport pixels, computed-style deltas, and design tokens).

HARD RULES — violating any of these fails the job:

1. USE CAPTURED TEXT VERBATIM for all application chrome: navigation items, labels, buttons, headers, column names, badges, empty states, tooltips. Never paraphrase, correct, or improve a string that appears in an extract. Casing in extracts is source casing — if a class list implies a CSS transform (e.g. `uppercase`), reproduce the transform, not the transformed text.
2. INVENT ONLY DATA: row contents, metrics, names, dates, chart series. Invented data must be story-shaped — trends up and to the right, believable company and person names, no empty states, no placeholder text like "test" or "lorem ipsum".
3. HONOR SUPPLIED GEOMETRY. Extract bounds are viewport pixels; reproduce layout dimensions and the spacing derived from them (sidebar widths, toolbar pitch, panel positions). Use the extract's design tokens (colors, font sizes, radii, spacing) verbatim.
4. ANCHOR TO ELEMENTS, NEVER TO RECORDED PIXELS. Anything that highlights or points at UI must resolve its position from the live DOM (getBoundingClientRect at paint time) via stable attributes you emit — never from coordinates recorded in the manifest.
5. NEVER INVENT A PAGE. The allowed route list in the task is the sandbox's entire navigable surface. Captured navigation items whose target is not in that list still render with their verbatim text but are inert: default cursor, no navigation, and on click a small one-line nudge ("This area isn't part of this demo").

OUTPUT SHAPE — validated mechanically after generation:

- Output exactly one complete HTML document starting with <!DOCTYPE html>. No markdown fences, no commentary before or after.
- Render each allowed route as <section data-route="ROUTE"> where ROUTE is the exact string from the allowed list. Every allowed route gets exactly one section; nothing else in the document may carry a data-route attribute.
- All in-sandbox navigation goes through elements carrying data-nav="ROUTE" with ROUTE from the allowed list; a small inline router toggles the matching section. The first route in the list is shown initially.
- Inert captured links carry data-inert (no data-nav, no href).
- The document is fully self-contained: all CSS in <style>, all behavior in inline <script>. No external requests of any kind — no src or href pointing at http(s) URLs, no @import, no url(http...), no fetch/XMLHttpRequest/WebSocket, no web fonts. Captured font families degrade to the closest system-font stack.
- Captured images become correctly-sized placeholder blocks using each image's captured dominant color — a plain block or a subtle CSS gradient with NOTHING written inside it. The words "missing", "unavailable", "placeholder", "no image", or any equivalent must never appear in visible copy anywhere in the document: a viewer must read an image region as content, never as an apology for content. Captured <canvas>/chart nodes become inline SVG charts with invented, plausible series using the captured palette.

INTERACTIVITY: the sandbox must feel real within each page. Tabs switch, filters filter, dropdowns open, inputs accept text, toggles toggle, buttons respond with plausible state changes. Prefer breadth of small working interactions over one deep flow."#;

struct PromptStep<'a> {
    index: usize,
    route: Option<&'a str>,
    action: Option<&'a str>,
    name: Option<&'a str>,
    screenshot: &'a [u8],
    screenshot_mime: &'static str,
    extract_json: Option<&'a [u8]>,
}

fn screenshot_mime(file: &str) -> &'static str {
    let ext = file.rsplit('.').next().unwrap_or_default();
    if ext.eq_ignore_ascii_case("png") {
        "image/png"
    } else if ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg") {
        "image/jpeg"
    } else {
        "image/webp"
    }
}

/// Pick which steps ride in the prompt when the recording is long: first pass
/// keeps the earliest step of each route (route coverage is what bounds the
/// output), second pass fills remaining slots in recording order.
fn select_steps<'a>(steps: &'a [PromptStep<'a>]) -> Vec<&'a PromptStep<'a>> {
    if steps.len() <= MAX_PROMPT_STEPS {
        return steps.iter().collect();
    }
    let mut selected: Vec<&PromptStep> = Vec::new();
    let mut seen_routes: Vec<&str> = Vec::new();
    for step in steps {
        if let Some(route) = step.route
            && !seen_routes.contains(&route)
        {
            seen_routes.push(route);
            selected.push(step);
        }
    }
    for step in steps {
        if selected.len() >= MAX_PROMPT_STEPS {
            break;
        }
        if !selected.iter().any(|s| s.index == step.index) {
            selected.push(step);
        }
    }
    selected.sort_by_key(|s| s.index);
    selected.truncate(MAX_PROMPT_STEPS);
    selected
}

/// Build the user-turn content blocks: one intro text block, then per selected
/// step a text block (metadata + extract JSON) followed by its screenshot.
fn build_user_content(
    manifest: &BundleManifest,
    steps: &[PromptStep<'_>],
    routes: &[String],
    title: &str,
    brief: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut blocks: Vec<serde_json::Value> = Vec::new();

    let routes_list = routes
        .iter()
        .map(|r| format!("- {r}"))
        .collect::<Vec<_>>()
        .join("\n");

    let brief = brief
        .filter(|b| !b.trim().is_empty())
        .map(|b| format!("\n\nBRIEF FROM THE OWNER:\n{b}"))
        .unwrap_or_default();

    let intro = format!(
        "Rebuild \"{title}\" as an interactive sandbox.\n\nViewport: {w}x{h}.\n\nALLOWED ROUTES (the complete navigable surface — one <section data-route> each, nothing beyond):\n{routes_list}{brief}\n\nBelow are the recorded steps in order. Each step gives its metadata, its DOM extract (exact structure, text, classes, bounds, tokens), and its screenshot. The extract is authoritative for text and styling; the screenshot is authoritative for overall visual appearance.",
        w = manifest.viewport.width,
        h = manifest.viewport.height,
    );
    blocks.push(serde_json::json!({ "type": "text", "text": intro }));

    for step in select_steps(steps) {
        let mut meta = format!("STEP {}", step.index + 1);
        if let Some(name) = step.name {
            meta.push_str(&format!(" — {name}"));
        }
        if let Some(route) = step.route {
            meta.push_str(&format!("\nroute: {route}"));
        }
        if let Some(action) = step.action {
            meta.push_str(&format!("\naction: {action}"));
        }
        match step.extract_json {
            Some(extract) => {
                meta.push_str("\nDOM extract:\n");
                meta.push_str(&String::from_utf8_lossy(extract));
            }
            None => meta.push_str("\n(no DOM extract for this step — rely on the screenshot)"),
        }
        blocks.push(serde_json::json!({ "type": "text", "text": meta }));

        let data = base64::engine::general_purpose::STANDARD.encode(step.screenshot);
        blocks.push(serde_json::json!({
            "type": "image",
            "source": { "type": "base64", "media_type": step.screenshot_mime, "data": data }
        }));
    }

    blocks
}

/// One generation call against the Claude API, streaming (a full sandbox is
/// far past the output size where a non-streaming request risks timeouts).
async fn call_claude(
    api_key: &str,
    model: &str,
    content: Vec<serde_json::Value>,
    verbose: bool,
) -> Result<String, CliError> {
    let body = serde_json::json!({
        "model": model,
        "max_tokens": 64000,
        "stream": true,
        "system": [{ "type": "text", "text": SYSTEM_PROMPT }],
        "messages": [{ "role": "user", "content": content }],
    });

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| CliError::Other(format!("HTTP client: {e}")))?;

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| CliError::Other(format!("Claude API request failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(CliError::Other(format!(
            "Claude API returned {status}: {}",
            detail.chars().take(300).collect::<String>()
        )));
    }

    // SSE parse. Newline bytes never occur inside UTF-8 continuation
    // sequences, so splitting the byte buffer at b'\n' is safe.
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::new();
    let mut text = String::new();
    let mut stop_reason: Option<String> = None;
    let mut output_tokens: Option<u64> = None;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| CliError::Other(format!("Claude API stream failed: {e}")))?;
        buf.extend_from_slice(&chunk);

        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end();
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<serde_json::Value>(data) else {
                continue;
            };
            match event.get("type").and_then(|t| t.as_str()) {
                Some("content_block_delta") => {
                    if let Some(delta) = event.get("delta")
                        && delta.get("type").and_then(|t| t.as_str()) == Some("text_delta")
                        && let Some(t) = delta.get("text").and_then(|t| t.as_str())
                    {
                        text.push_str(t);
                        if verbose && text.len() % 20000 < t.len() {
                            eprintln!("  … {} KB generated", text.len() / 1024);
                        }
                    }
                }
                Some("message_delta") => {
                    stop_reason = event
                        .get("delta")
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(|s| s.as_str())
                        .map(String::from);
                    output_tokens = event
                        .get("usage")
                        .and_then(|u| u.get("output_tokens"))
                        .and_then(|t| t.as_u64());
                }
                Some("error") => {
                    let msg = event
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown stream error");
                    return Err(CliError::Other(format!("Claude API error: {msg}")));
                }
                _ => {}
            }
        }
    }

    if verbose && let Some(tokens) = output_tokens {
        eprintln!("  {tokens} output tokens");
    }

    match stop_reason.as_deref() {
        Some("end_turn") => Ok(text),
        Some("max_tokens") => Err(CliError::Other(
            "Generation exceeded the output limit — try a shorter recording".into(),
        )),
        Some("refusal") => Err(CliError::Other(
            "The model declined to generate this sandbox".into(),
        )),
        other => Err(CliError::Other(format!(
            "Generation ended unexpectedly (stop_reason: {other:?})"
        ))),
    }
}

/// Generate through the user's local Claude Code (`claude -p`) instead of the
/// raw API. The bundle's extracts and screenshots are staged into a temp
/// workdir, the agent reads them itself and writes `sandbox.html` there, and
/// the CLI validates the result exactly as it does for the API path. No key
/// to manage — this rides the subscription the user already has.
async fn generate_via_agent(
    manifest: &BundleManifest,
    steps: &[PromptStep<'_>],
    routes: &[String],
    title: &str,
    brief: Option<&str>,
    model: Option<&str>,
    verbose: bool,
) -> Result<String, CliError> {
    let workdir = std::env::temp_dir().join(format!(
        "stepshots-sandbox-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    ));
    std::fs::create_dir_all(workdir.join("steps"))?;
    std::fs::create_dir_all(workdir.join("dom"))?;

    let selected = select_steps(steps);
    let mut step_lines = String::new();
    for step in &selected {
        let ext = match step.screenshot_mime {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            _ => "webp",
        };
        let shot = format!("steps/{}.{ext}", step.index);
        std::fs::write(workdir.join(&shot), step.screenshot)?;

        step_lines.push_str(&format!("\n## Step {}", step.index + 1));
        if let Some(name) = step.name {
            step_lines.push_str(&format!(" — {name}"));
        }
        step_lines.push('\n');
        if let Some(route) = step.route {
            step_lines.push_str(&format!("- route: {route}\n"));
        }
        if let Some(action) = step.action {
            step_lines.push_str(&format!("- action: {action}\n"));
        }
        step_lines.push_str(&format!("- screenshot: {shot}\n"));
        match step.extract_json {
            Some(extract) => {
                let dom = format!("dom/{}.json", step.index);
                std::fs::write(workdir.join(&dom), extract)?;
                step_lines.push_str(&format!("- DOM extract: {dom}\n"));
            }
            None => step_lines.push_str("- (no DOM extract — rely on the screenshot)\n"),
        }
    }

    let routes_list = routes
        .iter()
        .map(|r| format!("- {r}"))
        .collect::<Vec<_>>()
        .join("\n");
    let brief = brief
        .filter(|b| !b.trim().is_empty())
        .map(|b| format!("\n\nBRIEF FROM THE OWNER:\n{b}"))
        .unwrap_or_default();

    let task = format!(
        "{SYSTEM_PROMPT}\n\n---\n\nTASK: Rebuild \"{title}\" as an interactive sandbox.\n\nViewport: {w}x{h}.\n\nALLOWED ROUTES (the complete navigable surface — one <section data-route> each, nothing beyond):\n{routes_list}{brief}\n\nThe recorded steps are listed below. Read every step's DOM extract and view every screenshot before writing any HTML. The extract is authoritative for text and styling; the screenshot is authoritative for overall visual appearance.\n{step_lines}\nWhen you have studied all of them, write the complete HTML document to `sandbox.html` in this directory. Write only that file.",
        w = manifest.viewport.width,
        h = manifest.viewport.height,
    );
    std::fs::write(workdir.join("task.md"), &task)?;

    // Claude Code occasionally dies right at startup with a bare exit 1
    // (observed three times across generations); one automatic retry absorbs
    // that without the user re-running a 25-minute command by hand.
    let mut last_err = String::new();
    for attempt in 1..=2u8 {
        let mut cmd = tokio::process::Command::new("claude");
        cmd.current_dir(&workdir)
            .arg("-p")
            .arg("Read task.md in the current directory and do exactly what it says: study the referenced dom/*.json extracts and steps/* screenshots, then write the finished single-file sandbox to sandbox.html here. Do not print the HTML to stdout.")
            .arg("--allowedTools")
            .arg("Read,Write,Glob")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        if let Some(model) = model {
            cmd.arg("--model").arg(model);
        }

        let child = cmd
            .spawn()
            .map_err(|e| CliError::Other(format!("Could not launch Claude Code: {e}")))?;

        // A full generation is minutes of agent work; cap it well above that so a
        // hung agent cannot pin the terminal forever.
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(1500),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| CliError::Other("Claude Code timed out after 25 minutes".into()))?
        .map_err(|e| CliError::Other(format!("Claude Code failed: {e}")))?;

        if verbose {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if output.status.success() {
            let html = std::fs::read_to_string(workdir.join("sandbox.html")).map_err(|_| {
                CliError::Other(
                    "The agent finished without writing sandbox.html — try again, or use an API key (ANTHROPIC_API_KEY)".into(),
                )
            })?;
            let _ = std::fs::remove_dir_all(&workdir);
            return Ok(html);
        }

        // Claude Code reports its errors on STDOUT; stderr is usually empty.
        // Surface the tails of both or the failure reads as mute.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = [stderr.trim(), stdout.trim()]
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.chars().take(400).collect::<String>())
            .collect::<Vec<_>>()
            .join(" | ");
        last_err = format!("Claude Code exited with {}: {detail}", output.status);
        if attempt == 1 {
            eprintln!(
                "  Generation attempt failed ({}) — retrying once …",
                output.status
            );
        }
    }
    Err(CliError::Other(last_err))
}

/// Pull the HTML document out of the model's text, tolerating a stray fence.
fn extract_html(text: &str) -> Result<String, CliError> {
    let trimmed = text.trim();
    let lower = trimmed.to_ascii_lowercase();
    let start = lower
        .find("<!doctype html")
        .or_else(|| lower.find("<html"))
        .ok_or_else(|| CliError::Other("Generation produced no HTML document".into()))?;
    let mut html = &trimmed[start..];
    if let Some(end) = html.rfind("</html>") {
        html = &html[..end + "</html>".len()];
    }
    Ok(html.to_string())
}

/// The default model for the direct-API path. The agent path deliberately
/// takes no default — it runs on whatever the user's Claude Code is set to,
/// which is what their subscription actually covers.
const DEFAULT_MODEL: &str = "claude-opus-5";

/// Who runs the model for a generation.
enum Backend {
    /// Direct Claude API call with the user's `ANTHROPIC_API_KEY`.
    Api { api_key: String },
    /// The user's local Claude Code (`claude -p`) — no key to manage; rides
    /// the subscription they already have.
    Agent,
}

/// Key wins when present (explicit configuration beats convenience); `--agent`
/// forces the agent; otherwise fall through to the agent when Claude Code is
/// installed.
fn resolve_backend(force_agent: bool) -> Result<Backend, CliError> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|k| !k.is_empty());
    if !force_agent && let Some(api_key) = api_key {
        return Ok(Backend::Api { api_key });
    }
    if claude_agent_available() {
        return Ok(Backend::Agent);
    }
    Err(CliError::Other(
        "Sandbox generation runs on your machine — your recording's extracts never leave it. It needs either your own Claude API key (set ANTHROPIC_API_KEY) or Claude Code installed (the `claude` command), whose subscription covers the generation.".into(),
    ))
}

fn claude_agent_available() -> bool {
    std::process::Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `stepshots sandbox generate <bundle>` — build the sandbox artifact locally.
pub async fn generate(
    bundle_path: &Path,
    brief: Option<&str>,
    model: Option<&str>,
    out: Option<&Path>,
    force_agent: bool,
    json: bool,
    verbose: bool,
) -> Result<PathBuf, CliError> {
    let backend = resolve_backend(force_agent)?;

    let (manifest, screenshots, _transition_frames, dom_extracts) = read_bundle(bundle_path)?;
    let manifest = &manifest;

    if dom_extracts.is_empty() {
        return Err(CliError::Bundle(format!(
            "{} carries no DOM extracts — re-record with `stepshots record --dom`",
            bundle_path.display()
        )));
    }

    let mut routes: Vec<String> = Vec::new();
    for step in &manifest.steps {
        if let Some(ref path) = step.current_path
            && !routes.iter().any(|r| r == path)
        {
            routes.push(path.clone());
        }
    }
    if routes.is_empty() {
        return Err(CliError::Bundle(
            "The recording carries no route information (current_path) — re-record with a current stepshots CLI".into(),
        ));
    }

    let title = bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Sandbox");

    let steps: Vec<PromptStep> = manifest
        .steps
        .iter()
        .enumerate()
        .map(|(i, step)| PromptStep {
            index: i,
            route: step.current_path.as_deref(),
            action: step.action.as_deref(),
            name: step.name.as_deref(),
            screenshot: &screenshots[i],
            screenshot_mime: screenshot_mime(&step.file),
            extract_json: dom_extracts.get(&i).map(|v| v.as_slice()),
        })
        .collect();

    if !json {
        let via = match &backend {
            Backend::Api { .. } => format!("the Claude API ({})", model.unwrap_or(DEFAULT_MODEL)),
            Backend::Agent => "your local Claude Code agent".to_string(),
        };
        println!(
            "Generating sandbox from {} ({} steps, {} extract(s), {} route(s)) via {via} …",
            bundle_path.display(),
            steps.len(),
            dom_extracts.len(),
            routes.len(),
        );
    }

    let html = match backend {
        Backend::Api { api_key } => {
            let content = build_user_content(manifest, &steps, &routes, title, brief);
            let raw =
                call_claude(&api_key, model.unwrap_or(DEFAULT_MODEL), content, verbose).await?;
            extract_html(&raw)?
        }
        Backend::Agent => {
            generate_via_agent(manifest, &steps, &routes, title, brief, model, verbose).await?
        }
    };

    let violations = validate_sandbox_html(&html, &routes);
    if !violations.is_empty() {
        return Err(CliError::Other(format!(
            "Generated sandbox failed validation: {}. Generation varies run to run — try again.",
            violations.join("; ")
        )));
    }

    let out_path = out.map(Path::to_path_buf).unwrap_or_else(|| {
        bundle_path.with_file_name(format!(
            "{}.sandbox.html",
            bundle_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("sandbox")
        ))
    });
    std::fs::write(&out_path, &html)?;

    if json {
        println!(
            "{}",
            serde_json::json!({
                "output": out_path.display().to_string(),
                "bytes": html.len(),
                "routes": routes,
                "steps": steps.len(),
            })
        );
    } else {
        println!(
            "  Created: {} ({} KB)",
            out_path.display(),
            html.len() / 1024
        );
        println!("\nReview it in a browser, commit it, then publish with");
        println!(
            "  stepshots sandbox push {} --demo-id <id>",
            out_path.display()
        );
    }
    Ok(out_path)
}

/// `stepshots sandbox push <artifact> --demo-id <id>` — upload the reviewed
/// artifact. The server re-validates it against the demo's manifest (a pushed
/// artifact is client input) and hosts it as a draft.
pub async fn push(
    artifact_path: &Path,
    demo_id: &str,
    title: Option<&str>,
    json: bool,
    server_url: &str,
    token: &str,
) -> Result<(), CliError> {
    let html = std::fs::read_to_string(artifact_path)?;

    let client = reqwest::Client::new();
    let url = format!("{}/api/sandboxes/push", server_url.trim_end_matches('/'));
    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "demo_id": demo_id,
            "title": title,
            "html": html,
        }))
        .send()
        .await
        .map_err(|e| CliError::Upload(format!("Push failed: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(CliError::Upload(format!(
            "Push failed ({status}): {}",
            detail.chars().take(400).collect::<String>()
        )));
    }

    let result: serde_json::Value = response
        .json()
        .await
        .map_err(|e| CliError::Upload(format!("Invalid push response: {e}")))?;
    let sandbox_id = result
        .get("sandbox_id")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let view_url = format!(
        "{}/dashboard/sandboxes/{sandbox_id}",
        server_url.trim_end_matches('/')
    );

    if json {
        println!(
            "{}",
            serde_json::json!({ "sandbox_id": sandbox_id, "url": view_url })
        );
    } else {
        println!("  Pushed! Sandbox ID: {sandbox_id}");
        println!("  Review at: {view_url}");
    }
    Ok(())
}

/// Strip `dom/` extract sidecars from a bundle's bytes before upload, so the
/// structural map of the app never leaves the machine. Returns the original
/// bytes untouched when the bundle carries no extracts.
pub fn strip_dom_extracts(bundle_bytes: &[u8]) -> Result<Vec<u8>, CliError> {
    use std::io::{Cursor, Read as _, Write as _};
    use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

    let mut archive = ZipArchive::new(Cursor::new(bundle_bytes))
        .map_err(|e| CliError::Bundle(format!("Invalid bundle: {e}")))?;

    let has_dom = (0..archive.len()).any(|i| {
        archive
            .by_index_raw(i)
            .map(|e| e.name().starts_with("dom/"))
            .unwrap_or(false)
    });
    if !has_dom {
        return Ok(bundle_bytes.to_vec());
    }

    let mut out = Vec::new();
    {
        let mut writer = ZipWriter::new(Cursor::new(&mut out));
        let options = SimpleFileOptions::default();
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| CliError::Bundle(format!("Invalid bundle entry: {e}")))?;
            let name = entry.name().to_string();
            if name.starts_with("dom/") {
                continue;
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry
                .read_to_end(&mut bytes)
                .map_err(|e| CliError::Bundle(format!("Failed to read {name}: {e}")))?;
            if name == "manifest.json" {
                // Drop the dangling sidecar references along with the files.
                if let Ok(mut manifest) = serde_json::from_slice::<BundleManifest>(&bytes) {
                    for step in &mut manifest.steps {
                        step.dom = None;
                    }
                    bytes = serde_json::to_vec_pretty(&manifest)
                        .map_err(|e| CliError::Bundle(format!("Manifest rewrite: {e}")))?;
                }
            }
            writer
                .start_file(name, options)
                .map_err(|e| CliError::Bundle(format!("Zip write: {e}")))?;
            writer
                .write_all(&bytes)
                .map_err(|e| CliError::Bundle(format!("Zip write: {e}")))?;
        }
        writer
            .finish()
            .map_err(|e| CliError::Bundle(format!("Zip finish: {e}")))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_html_strips_fences_and_prose() {
        let text = "Here is the sandbox:\n```html\n<!DOCTYPE html><html><body>x</body></html>\n```";
        let html = extract_html(text).unwrap();
        assert!(html.starts_with("<!DOCTYPE html"));
        assert!(html.ends_with("</html>"));
    }

    #[test]
    fn strip_dom_extracts_removes_sidecars_and_refs() {
        use std::io::{Cursor, Write as _};
        use zip::{ZipWriter, write::SimpleFileOptions};

        let manifest = serde_json::json!({
            "version": 1,
            "title": "t",
            "viewport": { "width": 100, "height": 100 },
            "created_at": "2026-01-01T00:00:00Z",
            "steps": [{ "file": "steps/0.webp", "dom": "dom/0.json" }],
        });
        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            writer.start_file("manifest.json", options).unwrap();
            writer
                .write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
                .unwrap();
            writer.start_file("steps/0.webp", options).unwrap();
            writer.write_all(b"img").unwrap();
            writer.start_file("dom/0.json", options).unwrap();
            writer.write_all(b"{}").unwrap();
            writer.finish().unwrap();
        }

        let stripped = strip_dom_extracts(&buf).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(&stripped)).unwrap();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index_raw(i).unwrap().name().to_string())
            .collect();
        assert!(!names.iter().any(|n| n.starts_with("dom/")));
        assert!(names.contains(&"manifest.json".to_string()));

        let mut manifest_bytes = Vec::new();
        std::io::Read::read_to_end(
            &mut archive.by_name("manifest.json").unwrap(),
            &mut manifest_bytes,
        )
        .unwrap();
        assert!(!String::from_utf8_lossy(&manifest_bytes).contains("dom/0.json"));
    }

    #[test]
    fn strip_is_identity_without_extracts() {
        use std::io::{Cursor, Write as _};
        use zip::{ZipWriter, write::SimpleFileOptions};

        let mut buf = Vec::new();
        {
            let mut writer = ZipWriter::new(Cursor::new(&mut buf));
            writer
                .start_file("manifest.json", SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"{}").unwrap();
            writer.finish().unwrap();
        }
        assert_eq!(strip_dom_extracts(&buf).unwrap(), buf);
    }
}
