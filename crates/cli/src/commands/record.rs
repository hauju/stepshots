use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};
use manifest::{
    ArrowPointer, BundleManifest, BundleManifestStep, ElementBounds, HighlightEntry,
    HotspotIndicator, PopupIndicator, StepConfig, Viewport, ZoomRegion, resolve_viewport,
};

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::bundler::create_bundle;
use crate::config::{StepshotsConfig, TutorialConfig};
use crate::error::CliError;
use crate::output::{RecordOutput, StepOutput, TutorialOutput};

/// Record one or more tutorials into `.stepshot` bundles.
pub async fn run(
    config: &StepshotsConfig,
    tutorials: &[String],
    output_dir: &Path,
    dry_run: bool,
    json: bool,
) -> Result<(), CliError> {
    let selected: Vec<(&String, &TutorialConfig)> = if tutorials.is_empty() {
        config.tutorials.iter().collect()
    } else {
        let mut selected = Vec::new();
        for key in tutorials {
            let tut = config.tutorials.get(key).ok_or_else(|| {
                CliError::Config(format!(
                    "Tutorial '{key}' not found. Available: {}",
                    config
                        .tutorials
                        .keys()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
            })?;
            selected.push((key, tut));
        }
        selected
    };

    let mut tutorial_outputs: Vec<TutorialOutput> = Vec::new();

    for (key, tutorial) in &selected {
        if !json {
            println!("Recording: {} ({})", tutorial.title, key);
        }

        if dry_run {
            if !json {
                println!(
                    "  [dry-run] Would record {} steps → {}/{key}.stepshot",
                    tutorial.steps.len(),
                    output_dir.display()
                );
            }
            tutorial_outputs.push(TutorialOutput {
                key: key.to_string(),
                title: tutorial.title.clone(),
                output: Some(format!("{}/{key}.stepshot", output_dir.display())),
                steps_total: tutorial.steps.len(),
                steps_completed: None,
                steps: None,
            });
            continue;
        }

        let output_path = output_dir.join(format!("{key}.stepshot"));
        let effective_viewport = resolve_viewport(config.format.as_ref(), &config.viewport);
        let step_results =
            record_tutorial(config, tutorial, &effective_viewport, &output_path, json).await?;

        if !json {
            println!(
                "  Created: {} ({} steps)",
                output_path.display(),
                tutorial.steps.len()
            );
        }

        tutorial_outputs.push(TutorialOutput {
            key: key.to_string(),
            title: tutorial.title.clone(),
            output: Some(output_path.display().to_string()),
            steps_total: tutorial.steps.len(),
            steps_completed: Some(step_results.len()),
            steps: Some(step_results),
        });
    }

    if json {
        let out = RecordOutput {
            success: true,
            command: "record",
            dry_run: if dry_run { Some(true) } else { None },
            tutorials: Some(tutorial_outputs),
            error: None,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing RecordOutput")
        );
    }

    Ok(())
}

/// Record a single tutorial.
pub async fn record_tutorial(
    config: &StepshotsConfig,
    tutorial: &TutorialConfig,
    viewport: &Viewport,
    output_path: &Path,
    json: bool,
) -> Result<Vec<StepOutput>, CliError> {
    let browser = Browser::launch(viewport, true).await?;

    // Apply color scheme if configured
    if let Some(ref theme) = config.theme {
        browser.set_color_scheme(theme).await?;
    }

    // Navigate to the tutorial start page
    let start_url = resolve_url(&config.base_url, &tutorial.url);
    browser.navigate(&start_url).await?;
    browser.wait_idle(config.default_delay).await;

    let step_count = tutorial.steps.len();
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
    let mut manifest_steps: Vec<BundleManifestStep> = Vec::with_capacity(step_count);
    let mut step_results: Vec<StepOutput> = Vec::with_capacity(step_count);
    // Transition frames keyed by step index (0-based, matching screenshot index)
    let mut all_transition_frames: std::collections::HashMap<usize, Vec<Vec<u8>>> =
        std::collections::HashMap::new();

    // Execute each config step and capture the screenshot for that step's scene.
    for (i, step) in tutorial.steps.iter().enumerate() {
        pb.set_message(format!(
            "{}: {}",
            step.action,
            step.selector.as_deref().unwrap_or("")
        ));

        wait_for_step_target(&browser, step).await?;
        let capture_before_action = should_capture_before_action(step);
        restore_scene_scroll(&browser, step).await?;

        let (
            scene_url,
            step_highlight,
            step_blurs,
            step_arrows,
            step_hotspots,
            step_popups,
            step_zooms,
        ) = if capture_before_action {
            let scene_url = get_current_url(&browser).await;
            (
                scene_url,
                resolve_highlight(&browser, step, viewport, i + 1).await?,
                resolve_blur_regions(&browser, step, viewport, i + 1).await?,
                resolve_arrows(&browser, step, viewport, i + 1).await?,
                resolve_hotspots(&browser, step, viewport, i + 1).await?,
                resolve_popups(&browser, step, viewport, i + 1).await?,
                resolve_zoom_regions(&browser, step, viewport, i + 1).await?,
            )
        } else {
            (None, None, vec![], vec![], vec![], vec![], vec![])
        };

        if capture_before_action {
            let png = browser.screenshot().await?;
            screenshots.push(png);
        }

        // Execute the action (may capture transition frames for scroll steps)
        let action_result = execute_action(&browser, step, &config.base_url).await?;

        // For scroll actions, the smooth scroll + frame capture replaces the idle wait.
        // For other actions, wait for things to settle.
        if action_result.transition_frames.is_empty() {
            let delay = step.delay.unwrap_or(config.default_delay);
            browser.wait_idle(delay).await;
        }

        let (
            scene_url,
            step_highlight,
            step_blurs,
            step_arrows,
            step_hotspots,
            step_popups,
            step_zooms,
        ) = if capture_before_action {
            (
                scene_url,
                step_highlight,
                step_blurs,
                step_arrows,
                step_hotspots,
                step_popups,
                step_zooms,
            )
        } else {
            let scene_url = get_current_url(&browser).await;
            let overlays = (
                resolve_highlight(&browser, step, viewport, i + 1).await?,
                resolve_blur_regions(&browser, step, viewport, i + 1).await?,
                resolve_arrows(&browser, step, viewport, i + 1).await?,
                resolve_hotspots(&browser, step, viewport, i + 1).await?,
                resolve_popups(&browser, step, viewport, i + 1).await?,
                resolve_zoom_regions(&browser, step, viewport, i + 1).await?,
            );
            let png = browser.screenshot().await?;
            screenshots.push(png);
            (
                scene_url, overlays.0, overlays.1, overlays.2, overlays.3, overlays.4, overlays.5,
            )
        };

        // Build transition frame paths and store the frame data
        let step_idx = i;
        let transition_frame_paths: Option<Vec<String>> =
            if !action_result.transition_frames.is_empty() {
                let paths: Vec<String> = (0..action_result.transition_frames.len())
                    .map(|f| format!("transitions/{step_idx}/{f}.webp"))
                    .collect();
                all_transition_frames.insert(step_idx, action_result.transition_frames);
                Some(paths)
            } else {
                None
            };

        manifest_steps.push(BundleManifestStep {
            file: format!("steps/{step_idx}.webp"),
            name: step.name.clone(),
            action: Some(step.action.clone()),
            url: scene_url.clone(),
            current_path: scene_url
                .as_deref()
                .and_then(|url| url.strip_prefix(config.base_url.trim_end_matches('/')))
                .map(|path| {
                    if path.is_empty() {
                        "/".to_string()
                    } else {
                        path.to_string()
                    }
                }),
            target_url: step.url.clone(),
            selector: step.selector.clone(),
            selector_quality: step.selector_quality.clone(),
            highlights: step_highlight.map(|a| vec![a]),
            blur_regions: if step_blurs.is_empty() {
                None
            } else {
                Some(step_blurs)
            },
            arrows: if step_arrows.is_empty() {
                None
            } else {
                Some(step_arrows)
            },
            hotspots: if step_hotspots.is_empty() {
                None
            } else {
                Some(step_hotspots)
            },
            popups: if step_popups.is_empty() {
                None
            } else {
                Some(step_popups)
            },
            ctas: None,
            zoom_regions: if step_zooms.is_empty() {
                None
            } else {
                Some(step_zooms)
            },
            text: step.text.clone(),
            key: step.key.clone(),
            scroll_x: step.scroll_x,
            scroll_y: step.scroll_y,
            scene_scroll_x: step.scene_scroll_x,
            scene_scroll_y: step.scene_scroll_y,
            value: step.value.clone(),
            delay: step.delay,
            transition_frames: transition_frame_paths,
        });

        step_results.push(StepOutput {
            index: i,
            name: step.name.clone(),
            action: step.action.clone(),
            selector: step.selector.clone(),
            status: "ok",
            error: None,
        });

        pb.inc(1);
    }

    pb.finish_with_message("done");

    // Build manifest and bundle
    let manifest = BundleManifest {
        version: 1,
        viewport: viewport.clone(),
        base_url: Some(config.base_url.clone()),
        start_path: Some(tutorial.url.clone()),
        format: config.format.clone(),
        steps: manifest_steps,
    };

    create_bundle(&manifest, &screenshots, &all_transition_frames, output_path)?;

    Ok(step_results)
}

async fn wait_for_step_target(browser: &Browser, step: &StepConfig) -> Result<(), CliError> {
    let selector = match step.action.as_str() {
        "click"
            if step
                .highlights
                .first()
                .and_then(|h| h.bounds.as_ref())
                .is_some() =>
        {
            None
        }
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
                "Timed out waiting for selector '{selector}' before '{}' action",
                step.action
            )));
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
}

fn should_capture_before_action(step: &StepConfig) -> bool {
    matches!(step.action.as_str(), "click" | "navigate")
}

async fn get_current_url(browser: &Browser) -> Option<String> {
    browser
        .page()
        .evaluate("window.location.href")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
}

async fn restore_scene_scroll(browser: &Browser, step: &StepConfig) -> Result<(), CliError> {
    let x = step.scene_scroll_x.unwrap_or(0.0);
    let y = step.scene_scroll_y.unwrap_or(0.0);
    browser.set_scroll_position(x, y).await?;
    browser.wait_idle(50).await;
    Ok(())
}

/// Returns true if bounds are at least partially visible within the viewport.
fn is_bounds_visible(bounds: &ElementBounds, viewport: &Viewport) -> bool {
    bounds.x + bounds.width > 0.0
        && bounds.y + bounds.height > 0.0
        && bounds.x < viewport.width as f64
        && bounds.y < viewport.height as f64
        && bounds.width > 0.0
        && bounds.height > 0.0
}

/// Returns true if a point is within the viewport.
fn is_point_visible(x: f64, y: f64, viewport: &Viewport) -> bool {
    x >= 0.0 && y >= 0.0 && x <= viewport.width as f64 && y <= viewport.height as f64
}

/// Resolve highlight config into a HighlightEntry with element bounds.
async fn resolve_highlight(
    browser: &Browser,
    step: &manifest::StepConfig,
    viewport: &Viewport,
    step_num: usize,
) -> Result<Option<HighlightEntry>, CliError> {
    if step.highlights.is_empty() {
        return Ok(None);
    }
    let ann_cfg = &step.highlights[0];
    let explicit_bounds = ann_cfg.bounds.clone();
    let sel = step.highlight_selector.as_ref().or(step.selector.as_ref());
    let bounds = if explicit_bounds.is_some() {
        explicit_bounds
    } else if let Some(sel) = sel {
        browser.get_bounds(sel).await?
    } else {
        None
    };
    let bounds = bounds.unwrap_or(manifest::ElementBounds {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    });
    if !is_bounds_visible(&bounds, viewport) {
        let sel_str = sel.map(|s| s.as_str()).unwrap_or("(none)");
        eprintln!(
            "  \u{26a0} Step {step_num} \"{}\": highlight selector \"{sel_str}\" resolved off-screen, skipping",
            step.name.as_deref().unwrap_or("")
        );
        return Ok(None);
    }
    Ok(Some(HighlightEntry {
        bounds,
        callout: ann_cfg.callout.clone(),
        position: ann_cfg.position.clone(),
        arrow: ann_cfg.arrow,
        color: ann_cfg.color.clone(),
        border_width: None,
        shape: None,
        is_click_target: None,
        callout_offset: None,
        user_edited: None,
        callout_style: None,
        button_text: None,
        spotlight: None,
        spotlight_opacity: None,
        animation: Some("zoom-in".to_string()),
        delay: Some(150),
        duration: Some(450),
    }))
}

async fn resolve_blur_regions(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
) -> Result<Vec<ElementBounds>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.blur_regions {
        if let Some(bounds) = browser.get_bounds(&cfg.selector).await? {
            if is_bounds_visible(&bounds, viewport) {
                results.push(bounds);
            } else {
                eprintln!(
                    "  \u{26a0} Step {step_num} \"{}\": blur selector \"{}\" resolved off-screen, skipping",
                    step.name.as_deref().unwrap_or(""),
                    cfg.selector
                );
            }
        }
    }
    Ok(results)
}

async fn resolve_arrows(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
) -> Result<Vec<ArrowPointer>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.arrows {
        let from = browser.get_element_center(&cfg.from_selector).await?;
        let to = browser.get_element_center(&cfg.to_selector).await?;
        if let (Some(from), Some(to)) = (from, to) {
            if !is_point_visible(from.x, from.y, viewport)
                || !is_point_visible(to.x, to.y, viewport)
            {
                eprintln!(
                    "  \u{26a0} Step {step_num} \"{}\": arrow endpoint resolved off-screen, skipping",
                    step.name.as_deref().unwrap_or("")
                );
                continue;
            }
            results.push(ArrowPointer {
                from,
                to,
                color: cfg.color.clone(),
                stroke_width: cfg.stroke_width,
                curvature: cfg.curvature,
                text: None,
                font_size: None,
                animation: None,
                delay: None,
                duration: None,
            });
        }
    }
    Ok(results)
}

async fn resolve_hotspots(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
) -> Result<Vec<HotspotIndicator>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.hotspots {
        if let Some(center) = browser.get_element_center(&cfg.selector).await? {
            if !is_point_visible(center.x, center.y, viewport) {
                eprintln!(
                    "  \u{26a0} Step {step_num} \"{}\": hotspot selector \"{}\" resolved off-screen, skipping",
                    step.name.as_deref().unwrap_or(""),
                    cfg.selector
                );
                continue;
            }
            results.push(HotspotIndicator {
                x: center.x,
                y: center.y,
                color: cfg.color.clone(),
                size: cfg.size,
                callout: cfg.callout.clone(),
                position: cfg.position.clone(),
                is_click_target: cfg.is_click_target,
            });
        }
    }
    Ok(results)
}

async fn resolve_popups(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
) -> Result<Vec<PopupIndicator>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.popups {
        if let Some(center) = browser.get_element_center(&cfg.selector).await? {
            if !is_point_visible(center.x, center.y, viewport) {
                eprintln!(
                    "  \u{26a0} Step {step_num} \"{}\": popup selector \"{}\" resolved off-screen, skipping",
                    step.name.as_deref().unwrap_or(""),
                    cfg.selector
                );
                continue;
            }
            results.push(PopupIndicator {
                x: center.x,
                y: center.y,
                title: cfg.title.clone(),
                body: cfg.body.clone(),
                width: cfg.width,
                color: cfg.color.clone(),
                text_color: cfg.text_color.clone(),
                style: cfg.style.clone(),
                variant: cfg.variant.clone(),
                size: cfg.size.clone(),
                border_radius: None,
                animation: Some("fade-up".to_string()),
                delay: Some(150),
                duration: Some(450),
                dismissible: None,
                is_click_target: None,
                button_text: cfg.button_text.clone(),
                button_url: cfg.button_url.clone(),
                open_in_new_tab: cfg.open_in_new_tab,
            });
        }
    }
    Ok(results)
}

async fn resolve_zoom_regions(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
) -> Result<Vec<ZoomRegion>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.zoom_regions {
        if let Some(bounds) = browser.get_bounds(&cfg.selector).await? {
            if is_bounds_visible(&bounds, viewport) {
                results.push(ZoomRegion {
                    bounds,
                    magnification: cfg.magnification,
                    delay: cfg.delay,
                    duration: cfg.duration,
                });
            } else {
                eprintln!(
                    "  \u{26a0} Step {step_num} \"{}\": zoom selector \"{}\" resolved off-screen, skipping",
                    step.name.as_deref().unwrap_or(""),
                    cfg.selector
                );
            }
        }
    }
    Ok(results)
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
