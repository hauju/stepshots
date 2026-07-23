//! `stepshots tour check` — replay tour source files against a live app.
//!
//! The live gate for guided tours: starting at `--url`, each step's target is
//! resolved the way the player resolves it (CSS selector first, then the
//! text/aria fallback anchors), its advance action is performed (click / type
//! / select), and the next step is resolved against the resulting page. Drift
//! vocabulary mirrors `stepshots verify`:
//!
//! - **ok**    — the selector matched.
//! - **drift** — the selector missed but a fallback anchor matched (warn).
//! - **fail**  — neither resolved within the wait timeout.
//!
//! `--update-fallbacks` rewrites each selector-resolved step's fallback
//! anchors from the live element, so anchors stay fresh without re-recording.

use std::path::{Path, PathBuf};

use manifest::{StepConfig, TourAdvance, TourFallback, TourFile, default_viewport};

use crate::browser::Browser;
use crate::error::CliError;

use super::tour::{collect_tour_files, load_tour_file};
use super::verify::FailOn;

/// Attribute the resolve script tags the matched element with, so advance
/// actions can address it regardless of how it was found.
const TAG_SELECTOR: &str = "[data-stepshots-check]";

/// How long a step's target may take to appear — matches the player's
/// `waitTimeoutMs` default.
const WAIT_TIMEOUT_MS: u64 = 12_000;
const POLL_INTERVAL_MS: u64 = 250;

/// Per-step outcome of a check run.
struct StepResult {
    index: usize,
    selector: String,
    title: String,
    /// "ok" | "drift" | "fail" | "skipped"
    status: &'static str,
    detail: Option<String>,
}

struct TourResult {
    path: PathBuf,
    key: String,
    steps: Vec<StepResult>,
    /// Fallback anchors refreshed by `--update-fallbacks`.
    anchors_updated: usize,
}

/// What the in-page resolve script found.
struct Resolved {
    /// "selector" or "fallback"
    how: String,
    text: Option<String>,
    aria: Option<String>,
}

pub async fn run(
    paths: &[PathBuf],
    base_url: &str,
    fail_on: FailOn,
    update_fallbacks: bool,
    profile_dir: Option<&Path>,
    json: bool,
) -> Result<(), CliError> {
    let files = collect_tour_files(paths)?;
    if !json {
        println!("Checking {} tour(s) against {base_url}\n", files.len());
    }

    let mut results = Vec::with_capacity(files.len());
    for path in &files {
        let mut file = load_tour_file(path)
            .map_err(|e| CliError::Config(format!("{}: {e}", path.display())))?;
        let result = check_tour(&mut file, path, base_url, update_fallbacks, profile_dir).await?;
        if update_fallbacks && result.anchors_updated > 0 {
            std::fs::write(path, format!("{}\n", serde_json::to_string_pretty(&file)?))?;
        }
        if !json {
            print_tour_result(&result);
        }
        results.push(result);
    }

    let (ok, drift, fail): (usize, usize, usize) =
        results
            .iter()
            .flat_map(|r| &r.steps)
            .fold((0, 0, 0), |(o, d, f), s| match s.status {
                "ok" => (o + 1, d, f),
                "drift" => (o, d + 1, f),
                "fail" => (o, d, f + 1),
                _ => (o, d, f),
            });
    let should_fail = fail > 0 || (fail_on == FailOn::Warn && drift > 0);

    if json {
        let out = serde_json::json!({
            "success": !should_fail,
            "command": "tour check",
            "url": base_url,
            "tours": results.iter().map(|r| serde_json::json!({
                "path": r.path.display().to_string(),
                "key": r.key,
                "anchorsUpdated": r.anchors_updated,
                "steps": r.steps.iter().map(|s| serde_json::json!({
                    "index": s.index,
                    "selector": s.selector,
                    "title": s.title,
                    "status": s.status,
                    "detail": s.detail,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "summary": { "ok": ok, "drift": drift, "fail": fail },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing tour check output")
        );
    } else {
        println!("\n{ok} ok · {drift} drifted · {fail} broken");
    }

    if should_fail {
        return Err(CliError::Reported { code: 1 });
    }
    Ok(())
}

/// Replay one tour. Only browser-launch failures return `Err`; a start page
/// that no longer loads is itself a failed check, reported in the result.
async fn check_tour(
    file: &mut TourFile,
    path: &Path,
    base_url: &str,
    update_fallbacks: bool,
    profile_dir: Option<&Path>,
) -> Result<TourResult, CliError> {
    let viewport = default_viewport();
    let browser = Browser::launch(&viewport, true, profile_dir).await?;

    let mut steps: Vec<StepResult> = Vec::with_capacity(file.steps.len());
    let mut anchors_updated = 0;
    let mut broken = false;

    if let Err(e) = browser.navigate(base_url).await {
        broken = true;
        steps.push(StepResult {
            index: 0,
            selector: String::new(),
            title: "(start page)".into(),
            status: "fail",
            detail: Some(format!("start page failed to load: {e}")),
        });
    } else {
        browser.wait_idle(1000).await;
    }

    for i in 0..file.steps.len() {
        if broken {
            let step = &file.steps[i];
            steps.push(StepResult {
                index: i,
                selector: step.selector.clone(),
                title: step.title.clone(),
                status: "skipped",
                detail: None,
            });
            continue;
        }
        let step = file.steps[i].clone();

        // Explicit navigation hint for steps the replay can't reach through
        // the previous step's advance alone.
        if let Some(check_path) = step.check.as_ref().and_then(|c| c.path.as_deref()) {
            let url = resolve_url(base_url, check_path);
            if let Err(e) = browser.navigate(&url).await {
                broken = true;
                steps.push(StepResult {
                    index: i,
                    selector: step.selector.clone(),
                    title: step.title.clone(),
                    status: "fail",
                    detail: Some(format!("check.path '{check_path}' failed to load: {e}")),
                });
                continue;
            }
            browser.wait_idle(800).await;
        }

        let resolved = wait_for_target(&browser, &step.selector, step.fallback.as_ref()).await?;
        let Some(resolved) = resolved else {
            broken = true;
            steps.push(StepResult {
                index: i,
                selector: step.selector.clone(),
                title: step.title.clone(),
                status: "fail",
                detail: Some(format!(
                    "target not found within {}s (selector and fallback anchors both missed)",
                    WAIT_TIMEOUT_MS / 1000
                )),
            });
            continue;
        };

        let (status, detail) = if resolved.how == "selector" {
            ("ok", None)
        } else {
            let anchor = resolved
                .aria
                .as_deref()
                .map(|a| format!("aria \"{a}\""))
                .or_else(|| resolved.text.as_deref().map(|t| format!("text \"{t}\"")))
                .unwrap_or_else(|| "anchor".into());
            (
                "drift",
                Some(format!("selector missed — matched fallback {anchor}")),
            )
        };

        // Refresh anchors from the live element — only on selector hits, so a
        // drifted match never bakes possibly-wrong anchors into the file.
        if update_fallbacks && status == "ok" {
            let fresh = TourFallback {
                text: resolved.text.clone(),
                aria: resolved.aria.clone(),
            };
            let fresh = (!fresh.is_empty()).then_some(fresh);
            let current = &mut file.steps[i].fallback;
            if !fallbacks_equal(current.as_ref(), fresh.as_ref()) {
                *current = fresh;
                anchors_updated += 1;
            }
        }

        // Perform the advance the way a user would, so the next step resolves
        // against the page this step leads to.
        if let Err(e) = perform_advance(&browser, &step.advance, step_check_value(&step)).await {
            broken = true;
            steps.push(StepResult {
                index: i,
                selector: step.selector.clone(),
                title: step.title.clone(),
                status: "fail",
                detail: Some(format!("advance failed: {e}")),
            });
            continue;
        }
        browser.wait_idle(600).await;

        steps.push(StepResult {
            index: i,
            selector: step.selector.clone(),
            title: step.title.clone(),
            status,
            detail,
        });
    }

    Ok(TourResult {
        path: path.to_path_buf(),
        key: file.key.clone(),
        steps,
        anchors_updated,
    })
}

fn step_check_value(step: &manifest::TourFileStep) -> Option<String> {
    step.check.as_ref().and_then(|c| c.value.clone())
}

fn fallbacks_equal(a: Option<&TourFallback>, b: Option<&TourFallback>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.text == b.text && a.aria == b.aria,
        _ => false,
    }
}

/// Poll for the step target, selector first then fallback anchors, mirroring
/// the player's `resolveTarget`. The matched element is tagged with
/// `data-stepshots-check` so the advance can address it.
async fn wait_for_target(
    browser: &Browser,
    selector: &str,
    fallback: Option<&TourFallback>,
) -> Result<Option<Resolved>, CliError> {
    let js = resolve_script(selector, fallback)?;
    let start = std::time::Instant::now();
    loop {
        let result = browser
            .page()
            .evaluate(js.clone())
            .await
            .map_err(|e| CliError::Browser(format!("target resolution failed: {e}")))?;
        if let Ok(serde_json::Value::Object(obj)) = result.into_value::<serde_json::Value>() {
            return Ok(Some(Resolved {
                how: obj
                    .get("how")
                    .and_then(|v| v.as_str())
                    .unwrap_or("selector")
                    .to_string(),
                text: obj.get("text").and_then(|v| v.as_str()).map(str::to_string),
                aria: obj.get("aria").and_then(|v| v.as_str()).map(str::to_string),
            }));
        }
        if start.elapsed().as_millis() as u64 >= WAIT_TIMEOUT_MS {
            return Ok(None);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(POLL_INTERVAL_MS)).await;
    }
}

/// In-page resolution, kept in lockstep with the player: visible selector
/// match wins; else aria-label exact; else interactive-element text exact,
/// then contains — but only when the contains match is unambiguous.
fn resolve_script(selector: &str, fallback: Option<&TourFallback>) -> Result<String, CliError> {
    let lowered = |v: Option<&String>| -> Result<String, CliError> {
        match v.map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            Some(s) => Ok(serde_json::to_string(&s)?),
            None => Ok("null".into()),
        }
    };
    let aria = lowered(fallback.and_then(|f| f.aria.as_ref()))?;
    let text = lowered(fallback.and_then(|f| f.text.as_ref()))?;
    Ok(format!(
        r#"(() => {{
            const TAG = "data-stepshots-check";
            for (const t of document.querySelectorAll("[" + TAG + "]")) t.removeAttribute(TAG);
            const visible = (el) => {{
                const r = el.getBoundingClientRect();
                return r.width > 0 && r.height > 0;
            }};
            let el = null, how = null;
            try {{
                const c = document.querySelector({sel});
                if (c && visible(c)) {{ el = c; how = "selector"; }}
            }} catch (e) {{}}
            const wantAria = {aria};
            const wantText = {text};
            if (!el && wantAria) {{
                for (const c of document.querySelectorAll("[aria-label]")) {{
                    if ((c.getAttribute("aria-label") || "").trim().toLowerCase() === wantAria && visible(c)) {{
                        el = c; how = "fallback"; break;
                    }}
                }}
            }}
            if (!el && wantText) {{
                const partial = [];
                for (const c of document.querySelectorAll("a,button,[role='button'],[data-testid]")) {{
                    if (!visible(c)) continue;
                    const t = (c.textContent || "").replace(/\s+/g, " ").trim().toLowerCase();
                    if (!t) continue;
                    if (t === wantText) {{ el = c; how = "fallback"; break; }}
                    if (t.includes(wantText)) partial.push(c);
                }}
                if (!el && partial.length === 1) {{ el = partial[0]; how = "fallback"; }}
            }}
            if (!el) return null;
            el.setAttribute(TAG, "1");
            const raw = (el.textContent || "").replace(/\s+/g, " ").trim();
            return {{ how, text: raw ? raw.slice(0, 120) : null, aria: el.getAttribute("aria-label") || null }};
        }})()"#,
        sel = serde_json::to_string(selector)?,
    ))
}

/// Perform a step's advance on the tagged element. Click and type reuse the
/// recorder's action machinery (real CDP events, new-tab adoption); change is
/// a value-set + change event like the recorder's `select`, with a
/// first-option default when the file gives no `check.value`.
async fn perform_advance(
    browser: &Browser,
    advance: &TourAdvance,
    value: Option<String>,
) -> Result<(), CliError> {
    match advance {
        TourAdvance::Click => {
            let step = synthetic_step("click", None)?;
            crate::actions::execute_action(browser, &step, "").await?;
        }
        TourAdvance::Input => {
            let text = value.unwrap_or_else(|| "Stepshots".into());
            let step = synthetic_step("type", Some(&text))?;
            crate::actions::execute_action(browser, &step, "").await?;
        }
        TourAdvance::Change => {
            let val = match value {
                Some(v) => serde_json::to_string(&v)?,
                None => "null".into(),
            };
            let js = format!(
                r#"(() => {{
                    const el = document.querySelector("{TAG_SELECTOR}");
                    if (!el) return "gone";
                    let val = {val};
                    if (val === null) {{
                        if (!el.options || el.options.length === 0) return "no-options";
                        const i = el.options.length > 1 && el.selectedIndex === 0 ? 1 : 0;
                        val = el.options[i].value;
                    }}
                    el.value = val;
                    el.dispatchEvent(new Event("change", {{ bubbles: true }}));
                    return "ok";
                }})()"#
            );
            let result = browser
                .page()
                .evaluate(js)
                .await
                .map_err(|e| CliError::Action(format!("change advance failed: {e}")))?;
            match result.into_value::<String>().ok().as_deref() {
                Some("ok") => {}
                Some("no-options") => {
                    return Err(CliError::Action(
                        "change step has no options to pick — set check.value".into(),
                    ));
                }
                _ => return Err(CliError::Action("change target disappeared".into())),
            }
        }
    }
    Ok(())
}

/// A minimal `StepConfig` targeting the tagged element, so `execute_action`
/// drives clicks and typing exactly like `record`/`verify` do.
fn synthetic_step(action: &str, text: Option<&str>) -> Result<StepConfig, CliError> {
    let mut v = serde_json::json!({ "action": action, "selector": TAG_SELECTOR });
    if let Some(text) = text {
        v["text"] = text.into();
    }
    serde_json::from_value(v).map_err(CliError::from)
}

fn resolve_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        let base = base.trim_end_matches('/');
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{base}{path}")
    }
}

fn print_tour_result(result: &TourResult) {
    let total = result.steps.len();
    println!("{} ({})", result.key, result.path.display());
    for s in &result.steps {
        let n = s.index + 1;
        let mark = match s.status {
            "ok" => "✓",
            "drift" => "⚠",
            "fail" => "✗",
            _ => "→",
        };
        let detail = s
            .detail
            .as_deref()
            .map(|d| format!(" — {d}"))
            .unwrap_or_else(|| {
                if s.status == "skipped" {
                    " — skipped".into()
                } else {
                    String::new()
                }
            });
        println!(
            "  {mark} {n}/{total} {} \"{}\"{detail}",
            s.selector, s.title
        );
    }
    if result.anchors_updated > 0 {
        println!(
            "  updated {} fallback anchor(s) in {}",
            result.anchors_updated,
            result.path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_script_embeds_lowered_anchors() {
        let fallback = TourFallback {
            text: Some("  New Project ".into()),
            aria: None,
        };
        let js = resolve_script("#go", Some(&fallback)).unwrap();
        assert!(js.contains(r#"const wantText = "new project";"#));
        assert!(js.contains("const wantAria = null;"));
        assert!(js.contains(r##"document.querySelector("#go")"##));
    }

    #[test]
    fn synthetic_steps_target_the_tagged_element() {
        let step = synthetic_step("type", Some("hello")).unwrap();
        assert_eq!(step.action, "type");
        assert_eq!(step.selector.as_deref(), Some(TAG_SELECTOR));
        assert_eq!(step.text.as_deref(), Some("hello"));
    }

    #[test]
    fn resolve_url_joins_paths_like_the_recorder() {
        assert_eq!(resolve_url("https://x.test/", "/a"), "https://x.test/a");
        assert_eq!(resolve_url("https://x.test", "a"), "https://x.test/a");
        assert_eq!(
            resolve_url("https://x.test", "https://y.test/b"),
            "https://y.test/b"
        );
    }
}
