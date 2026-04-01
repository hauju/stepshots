use std::path::Path;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use manifest::{BundleManifest, BundleManifestStep, HighlightEntry, StepConfig};

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::bundle_reader::read_bundle_manifest;
use crate::bundler::create_bundle;
use crate::error::CliError;

/// Re-record a `.stepshot` bundle with fresh screenshots and updated bounds.
pub async fn run(
    bundle_path: &Path,
    base_url_override: Option<&str>,
    output_dir: &Path,
    headed: bool,
    default_delay: u64,
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

    println!("Re-recording from: {}", style(bundle_path.display()).cyan());
    println!("  Base URL: {base_url}");
    println!("  Steps: {}", manifest.steps.len());

    let browser = Browser::launch(&manifest.viewport, !headed).await?;

    let step_count = manifest.steps.len();
    let pb = ProgressBar::new(step_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:30}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );

    let mut screenshots: Vec<Vec<u8>> = Vec::with_capacity(step_count);
    let mut new_steps: Vec<BundleManifestStep> = Vec::with_capacity(step_count);
    let mut failed_count: usize = 0;

    // Step 0: navigate to start URL and capture initial screenshot
    let step0 = &manifest.steps[0];
    let start_url = step0
        .url
        .as_deref()
        .ok_or_else(|| CliError::Bundle("Step 0 has no URL".into()))?;

    let start_url = resolve_url(&base_url, start_url);
    browser.navigate(&start_url).await?;
    browser.wait_idle(default_delay).await;

    let png = browser.screenshot().await?;
    screenshots.push(png);

    let current_url = get_current_url(&browser).await;
    new_steps.push(BundleManifestStep {
        file: "steps/0.webp".into(),
        action: None,
        url: current_url,
        selector: step0.selector.clone(),
        highlights: step0.highlights.clone(),
        blur_regions: step0.blur_regions.clone(),
        arrows: step0.arrows.clone(),
        hotspots: step0.hotspots.clone(),
        popups: step0.popups.clone(),
        ctas: step0.ctas.clone(),
        zoom_regions: step0.zoom_regions.clone(),
        text: None,
        key: None,
        scroll_x: None,
        scroll_y: None,
        value: None,
        delay: None,
        transition_frames: None,
    });
    pb.set_message("initial");
    pb.inc(1);

    // Steps 1..N: replay actions
    for (i, old_step) in manifest.steps.iter().enumerate().skip(1) {
        let action_name = old_step.action.as_deref().unwrap_or("unknown");
        let selector_display = old_step.selector.as_deref().unwrap_or("");
        pb.set_message(format!("{action_name}: {selector_display}"));

        // Capture new bounds BEFORE executing the action
        let new_bounds = if let Some(ref sel) = old_step.selector {
            browser.get_bounds(sel).await.unwrap_or(None)
        } else {
            None
        };

        // Convert manifest step to StepConfig and execute
        let step_config = StepConfig::from(old_step);
        let step_failed = match execute_action(&browser, &step_config, &base_url).await {
            Ok(_) => false,
            Err(e) => {
                failed_count += 1;
                pb.suspend(|| {
                    eprintln!(
                        "  {} Step {}/{} FAILED: {} on {:?} — {}",
                        style("⚠").yellow().bold(),
                        i,
                        step_count - 1,
                        action_name,
                        selector_display,
                        e
                    );
                    eprintln!("    Capturing current page state...");
                });
                true
            }
        };

        // Wait for things to settle
        let delay = old_step.delay.unwrap_or(default_delay);
        browser.wait_idle(delay).await;

        // Capture screenshot (even on failure)
        let png = browser.screenshot().await?;
        screenshots.push(png);

        let current_url = get_current_url(&browser).await;

        // Carry highlights with updated bounds
        let highlights = old_step.highlights.as_ref().map(|anns| {
            anns.iter()
                .map(|a| carry_highlight(a, new_bounds.clone()))
                .collect()
        });

        new_steps.push(BundleManifestStep {
            file: format!("steps/{i}.webp"),
            action: old_step.action.clone(),
            url: if step_failed {
                old_step.url.clone()
            } else {
                current_url
            },
            selector: old_step.selector.clone(),
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

    // Print summary
    let file_size = std::fs::metadata(&output_path)
        .map(|m| format_size(m.len()))
        .unwrap_or_else(|_| "?".into());

    let ok_count = step_count - 1 - failed_count; // -1 for step 0 which has no action
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

    Ok(())
}

/// Carry over highlight metadata with updated element bounds.
fn carry_highlight(
    old: &HighlightEntry,
    new_bounds: Option<manifest::ElementBounds>,
) -> HighlightEntry {
    HighlightEntry {
        bounds: new_bounds.unwrap_or_else(|| old.bounds.clone()),
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
    }
}

/// Validate that manifest steps have enough data for replay.
fn validate_replayability(manifest: &BundleManifest) -> Result<(), CliError> {
    for (i, step) in manifest.steps.iter().enumerate().skip(1) {
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
    let url = manifest
        .steps
        .first()
        .and_then(|s| s.url.as_deref())
        .ok_or_else(|| CliError::Bundle("No URL in step 0 to derive base URL".into()))?;

    // Parse scheme + host from the URL
    if let Some(idx) = url.find("://") {
        let rest = &url[idx + 3..];
        if let Some(path_start) = rest.find('/') {
            return Ok(url[..idx + 3 + path_start].to_string());
        }
        return Ok(url.to_string());
    }

    Err(CliError::Bundle(format!(
        "Cannot extract base URL from step 0 URL: {url}. Use --base-url to specify."
    )))
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
