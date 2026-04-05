use std::path::Path;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use manifest::{BundleManifest, BundleManifestStep, HighlightEntry, StepConfig, Viewport};

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::bundle_reader::read_bundle_manifest;
use crate::bundler::create_bundle;
use crate::error::CliError;
use crate::output::{RerecordOutput, StepOutput};

/// Re-record a `.stepshot` bundle with fresh screenshots and updated bounds.
pub async fn run(
    bundle_path: &Path,
    base_url_override: Option<&str>,
    output_dir: &Path,
    headed: bool,
    default_delay: u64,
    json: bool,
) -> Result<(), CliError> {
    let manifest = read_bundle_manifest(bundle_path)?;

    // Validate that steps have enough data to replay
    validate_replayability(&manifest)?;

    // Determine base URL from first step or override
    let base_url = if let Some(url) = base_url_override {
        url.to_string()
    } else {
        extract_base_url(&manifest)?
    };

    if !json {
        println!("Re-recording from: {}", style(bundle_path.display()).cyan());
        println!("  Base URL: {base_url}");
        println!("  Steps: {}", manifest.steps.len());
    }

    let browser = Browser::launch(&manifest.viewport, !headed).await?;

    let step_count = manifest.steps.len();
    let pb = if json {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new(step_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  [{bar:30}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        pb
    };

    let mut screenshots: Vec<Vec<u8>> = Vec::with_capacity(step_count);
    let mut new_steps: Vec<BundleManifestStep> = Vec::with_capacity(step_count);
    let mut step_results: Vec<StepOutput> = Vec::with_capacity(step_count);
    let mut failed_count: usize = 0;

    let start_path = manifest
        .start_path
        .as_deref()
        .or_else(|| manifest.steps.first().and_then(|s| s.current_path.as_deref()))
        .or_else(|| manifest.steps.first().and_then(|s| s.url.as_deref()))
        .ok_or_else(|| CliError::Bundle("Could not determine start URL".into()))?;

    let start_url = resolve_url(&base_url, start_path);
    browser.navigate(&start_url).await?;
    browser.wait_idle(default_delay).await;

    for (i, old_step) in manifest.steps.iter().enumerate() {
        let action_name = old_step.action.as_deref().unwrap_or("unknown");
        let selector_display = old_step.selector.as_deref().unwrap_or("");
        pb.set_message(format!("{action_name}: {selector_display}"));

        wait_for_replay_target(&browser, old_step).await?;
        let capture_before_action = should_capture_before_replay(old_step);
        restore_replay_scene_scroll(&browser, old_step).await?;
        let new_bounds = if capture_before_action {
            if let Some(ref sel) = old_step.selector {
                browser.get_bounds(sel).await.unwrap_or(None)
            } else {
                None
            }
        } else {
            None
        };
        let scene_url = if capture_before_action {
            get_current_url(&browser).await
        } else {
            None
        };
        let scene_highlights = if capture_before_action {
            old_step.highlights.as_ref().map(|anns| {
                anns.iter()
                    .filter_map(|a| carry_highlight(a, new_bounds.clone(), &manifest.viewport, i))
                    .collect()
            })
        } else {
            None
        };
        if capture_before_action {
            let png = browser.screenshot().await?;
            screenshots.push(png);
        }

        // Convert manifest step to StepConfig and execute
        let step_config = StepConfig::from(old_step);
        let (step_failed, step_error) =
            match execute_action(&browser, &step_config, &base_url).await {
                Ok(_) => (false, None),
                Err(e) => {
                    failed_count += 1;
                    let error_msg = e.to_string();
                    if !json {
                        pb.suspend(|| {
                            eprintln!(
                                "  {} Step {}/{} FAILED: {} on {:?} — {}",
                                style("⚠").yellow().bold(),
                                i,
                                step_count - 1,
                                action_name,
                                selector_display,
                                error_msg
                            );
                            eprintln!("    Capturing current page state...");
                        });
                    }
                    (true, Some(error_msg))
                }
            };

        step_results.push(StepOutput {
            index: i,
            name: old_step.name.clone(),
            action: action_name.to_string(),
            selector: old_step.selector.clone(),
            status: if step_failed { "failed" } else { "ok" },
            error: step_error,
        });

        // Wait for things to settle
        let delay = old_step.delay.unwrap_or(default_delay);
        browser.wait_idle(delay).await;

        let (scene_url, highlights) = if capture_before_action {
            (scene_url, scene_highlights)
        } else {
            let png = browser.screenshot().await?;
            screenshots.push(png);
            let current_url = get_current_url(&browser).await;
            let new_bounds = if let Some(ref sel) = old_step.selector {
                browser.get_bounds(sel).await.unwrap_or(None)
            } else {
                None
            };
            let highlights = old_step.highlights.as_ref().map(|anns| {
                anns.iter()
                    .filter_map(|a| carry_highlight(a, new_bounds.clone(), &manifest.viewport, i))
                    .collect()
            });
            (current_url, highlights)
        };

        new_steps.push(BundleManifestStep {
            file: format!("steps/{i}.webp"),
            name: old_step.name.clone(),
            action: old_step.action.clone(),
            url: if step_failed {
                old_step.url.clone()
            } else {
                scene_url
            },
            current_path: old_step.current_path.clone(),
            target_url: old_step.target_url.clone(),
            selector: old_step.selector.clone(),
            selector_quality: old_step.selector_quality.clone(),
            highlights,
            blur_regions: old_step.blur_regions.clone(),
            arrows: old_step.arrows.clone(),
            hotspots: old_step.hotspots.clone(),
            popups: old_step.popups.clone(),
            ctas: old_step.ctas.clone(),
            zoom_regions: old_step.zoom_regions.clone(),
            text: old_step.text.clone(),
            key: old_step.key.clone(),
            scroll_x: old_step.scroll_x,
            scroll_y: old_step.scroll_y,
            scene_scroll_x: old_step.scene_scroll_x,
            scene_scroll_y: old_step.scene_scroll_y,
            value: old_step.value.clone(),
            delay: old_step.delay,
            transition_frames: None,
        });

        pb.inc(1);
    }

    pb.finish_with_message("done");

    // Build output bundle
    let new_manifest = BundleManifest {
        version: manifest.version,
        viewport: manifest.viewport,
        base_url: manifest.base_url,
        start_path: manifest.start_path,
        format: manifest.format,
        steps: new_steps,
    };

    let bundle_name = bundle_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("re-recorded");
    let date = chrono::Local::now().format("%Y-%m-%d");
    let output_path = output_dir.join(format!("{bundle_name}-{date}.stepshot"));

    create_bundle(
        &new_manifest,
        &screenshots,
        &std::collections::HashMap::new(),
        &output_path,
    )?;

    // Output results
    let ok_count = step_count.saturating_sub(failed_count);

    if json {
        let out = RerecordOutput {
            success: failed_count == 0,
            command: "rerecord",
            source_bundle: bundle_path.display().to_string(),
            output: output_path.display().to_string(),
            steps_total: step_count,
            steps_completed: ok_count,
            steps_failed: failed_count,
            steps: step_results,
        };
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
    } else {
        let file_size = std::fs::metadata(&output_path)
            .map(|m| format_size(m.len()))
            .unwrap_or_else(|_| "?".into());

        println!();
        if failed_count == 0 {
            println!(
                "  {} {} → {}",
                style("✓").green().bold(),
                bundle_name,
                style(output_path.display()).cyan(),
            );
            println!("    {} steps · {}", step_count, file_size,);
        } else {
            println!(
                "  {} {} → {}",
                style("⚠").yellow().bold(),
                bundle_name,
                style(output_path.display()).cyan(),
            );
            println!(
                "    {} steps ({} ok, {} {}) · {}",
                step_count,
                ok_count,
                failed_count,
                style("failed").red(),
                file_size,
            );
        }
    }

    if failed_count > 0 {
        return Err(CliError::PartialSuccess(format!(
            "{failed_count} step(s) failed during re-recording"
        )));
    }

    Ok(())
}

/// Carry over highlight metadata with updated element bounds.
/// Returns `None` if the resolved bounds are entirely off-screen.
fn carry_highlight(
    old: &HighlightEntry,
    new_bounds: Option<manifest::ElementBounds>,
    viewport: &Viewport,
    step_num: usize,
) -> Option<HighlightEntry> {
    let bounds = new_bounds.unwrap_or_else(|| old.bounds.clone());
    if bounds.x + bounds.width <= 0.0
        || bounds.y + bounds.height <= 0.0
        || bounds.x >= viewport.width as f64
        || bounds.y >= viewport.height as f64
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        eprintln!("  \u{26a0} Step {step_num}: highlight resolved off-screen, skipping");
        return None;
    }
    Some(HighlightEntry {
        bounds,
        callout: old.callout.clone(),
        position: old.position.clone(),
        arrow: old.arrow,
        color: old.color.clone(),
        border_width: old.border_width,
        icon: old.icon.clone(),
        shape: old.shape.clone(),
        is_click_target: old.is_click_target,
        callout_offset: if old.user_edited == Some(true) {
            old.callout_offset.clone() // preserve manual positioning
        } else {
            None // auto-position recalculates from new bounds
        },
        user_edited: old.user_edited,
        callout_style: old.callout_style.clone(),
        button_text: old.button_text.clone(),
        spotlight: old.spotlight,
        animation: old.animation.clone(),
        delay: old.delay,
        duration: old.duration,
    })
}

/// Validate that manifest steps have enough data for replay.
fn validate_replayability(manifest: &BundleManifest) -> Result<(), CliError> {
    for (i, step) in manifest.steps.iter().enumerate() {
        if step.action.is_none() {
            return Err(CliError::Bundle(format!(
                "Step {i} has no action. This rerecord flow expects explicit action-owned steps."
            )));
        }
        let action = step.action.as_deref().unwrap_or("");
        match action {
            "type" if step.text.is_none() => {
                return Err(CliError::Bundle(format!(
                    "Step {i} (type on {:?}) is missing `text`. \
                     This bundle was created before re-record support. \
                     Re-record from config first.",
                    step.selector.as_deref().unwrap_or("?")
                )));
            }
            "key" if step.key.is_none() => {
                return Err(CliError::Bundle(format!(
                    "Step {i} (key) is missing `key`. \
                     Re-record from config first.",
                )));
            }
            "select" if step.value.is_none() => {
                return Err(CliError::Bundle(format!(
                    "Step {i} (select on {:?}) is missing `value`. \
                     Re-record from config first.",
                    step.selector.as_deref().unwrap_or("?")
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Extract the base URL (scheme + host) from the first step's URL.
fn extract_base_url(manifest: &BundleManifest) -> Result<String, CliError> {
    if let Some(base_url) = manifest.base_url.as_ref() {
        return Ok(base_url.clone());
    }
    Err(CliError::Bundle(
        "Bundle is missing base_url. Re-export or provide --base-url.".into(),
    ))
}

async fn wait_for_replay_target(
    browser: &Browser,
    step: &BundleManifestStep,
) -> Result<(), CliError> {
    let selector = match step.action.as_deref().unwrap_or("") {
        "click" if step
            .highlights
            .as_ref()
            .and_then(|highlights| highlights.first())
            .is_some() => None,
        "click" | "type" | "select" | "hover" => step.selector.as_deref(),
        _ => None,
    };

    let Some(selector) = selector else {
        return Ok(());
    };

    let timeout = std::time::Duration::from_secs(10);
    let start = std::time::Instant::now();
    loop {
        if browser.page().find_element(selector).await.is_ok() {
            return Ok(());
        }
        if start.elapsed() > timeout {
            return Err(CliError::Action(format!(
                "Timed out waiting for selector '{selector}' before replaying '{}'",
                step.action.as_deref().unwrap_or("unknown")
            )));
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
}

fn should_capture_before_replay(step: &BundleManifestStep) -> bool {
    matches!(step.action.as_deref(), Some("click" | "navigate"))
}

async fn restore_replay_scene_scroll(
    browser: &Browser,
    step: &BundleManifestStep,
) -> Result<(), CliError> {
    let x = step.scene_scroll_x.unwrap_or(0.0);
    let y = step.scene_scroll_y.unwrap_or(0.0);
    browser.set_scroll_position(x, y).await?;
    browser.wait_idle(50).await;
    Ok(())
}

fn resolve_url(base: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        let base = base.trim_end_matches('/');
        let url = if url.starts_with('/') {
            url.to_string()
        } else {
            format!("/{url}")
        };
        format!("{base}{url}")
    }
}

async fn get_current_url(browser: &Browser) -> Option<String> {
    browser
        .page()
        .evaluate("window.location.href")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
