use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};
use manifest::{
    ArrowPointer, BundleManifest, BundleManifestStep, CtaButton, ElementBounds, HighlightEntry,
    HotspotIndicator, PopupIndicator, StepConfig, Viewport, ZoomRegion, resolve_viewport,
};

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::bundler::create_bundle;
use crate::config::{StepshotsConfig, TutorialConfig};
use crate::error::CliError;

/// Record one or more tutorials into `.stepshot` bundles.
pub async fn run(
    config: &StepshotsConfig,
    tutorials: &[String],
    output_dir: &Path,
    dry_run: bool,
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

    for (key, tutorial) in &selected {
        println!("Recording: {} ({})", tutorial.title, key);

        if dry_run {
            println!(
                "  [dry-run] Would record {} steps → {}/{key}.stepshot",
                tutorial.steps.len(),
                output_dir.display()
            );
            continue;
        }

        let output_path = output_dir.join(format!("{key}.stepshot"));
        let effective_viewport = resolve_viewport(config.format.as_ref(), &config.viewport);
        record_tutorial(config, tutorial, &effective_viewport, &output_path).await?;

        println!(
            "  Created: {} ({} steps)",
            output_path.display(),
            tutorial.steps.len() + 1
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
) -> Result<(), CliError> {
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
    let pb = ProgressBar::new((step_count + 1) as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("  [{bar:30}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=> "),
    );

    let mut screenshots: Vec<Vec<u8>> = Vec::with_capacity(step_count + 1);
    let mut manifest_steps: Vec<BundleManifestStep> = Vec::with_capacity(step_count + 1);
    // Transition frames keyed by step index (1-based, matching screenshot index)
    let mut all_transition_frames: std::collections::HashMap<usize, Vec<Vec<u8>>> =
        std::collections::HashMap::new();

    // Screenshot 0: initial state (no overlays — step overlays are resolved
    // after each step's action in the loop below)
    let png = browser.screenshot().await?;
    screenshots.push(png);

    let current_url = browser
        .page()
        .evaluate("window.location.href")
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok());

    manifest_steps.push(BundleManifestStep {
        file: "steps/0.webp".into(),
        action: None,
        url: current_url,
        selector: None,
        highlights: None,
        blur_regions: None,
        arrows: None,
        hotspots: None,
        popups: None,
        ctas: None,
        zoom_regions: None,
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

    // Execute each config step and capture screenshot after.
    // Each step's overlays are resolved AFTER its action executes, so scrolls
    // and navigations bring the target element into view before bounds are read.
    for (i, step) in tutorial.steps.iter().enumerate() {
        pb.set_message(format!(
            "{}: {}",
            step.action,
            step.selector.as_deref().unwrap_or("")
        ));

        // Execute the action (may capture transition frames for scroll steps)
        let action_result = execute_action(&browser, step, &config.base_url).await?;

        // For scroll actions, the smooth scroll + frame capture replaces the idle wait.
        // For other actions, wait for things to settle.
        if action_result.transition_frames.is_empty() {
            let delay = step.delay.unwrap_or(config.default_delay);
            browser.wait_idle(delay).await;
        }

        // Resolve THIS step's overlays after its action (element is now in view)
        let (
            step_highlight,
            step_blurs,
            step_arrows,
            step_hotspots,
            step_popups,
            step_ctas,
            step_zooms,
        ) = (
            resolve_highlight(&browser, step).await?,
            resolve_blur_regions(&browser, step).await?,
            resolve_arrows(&browser, step).await?,
            resolve_hotspots(&browser, step).await?,
            resolve_popups(&browser, step).await?,
            resolve_ctas(&browser, step).await?,
            resolve_zoom_regions(&browser, step).await?,
        );

        // Screenshot after action
        let png = browser.screenshot().await?;
        screenshots.push(png);

        let current_url = browser
            .page()
            .evaluate("window.location.href")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok());

        // Build transition frame paths and store the frame data
        let step_idx = i + 1;
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
            action: Some(step.action.clone()),
            url: current_url,
            selector: step.selector.clone(),
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
            ctas: if step_ctas.is_empty() {
                None
            } else {
                Some(step_ctas)
            },
            zoom_regions: if step_zooms.is_empty() {
                None
            } else {
                Some(step_zooms)
            },
            text: step.text.clone(),
            key: step.key.clone(),
            scroll_x: step.scroll_x,
            scroll_y: step.scroll_y,
            value: step.value.clone(),
            delay: step.delay,
            transition_frames: transition_frame_paths,
        });

        pb.inc(1);
    }

    pb.finish_with_message("done");

    // Build manifest and bundle
    let manifest = BundleManifest {
        version: 1,
        viewport: viewport.clone(),
        format: config.format.clone(),
        steps: manifest_steps,
    };

    create_bundle(&manifest, &screenshots, &all_transition_frames, output_path)?;

    Ok(())
}

/// Resolve highlight config into a HighlightEntry with element bounds.
async fn resolve_highlight(
    browser: &Browser,
    step: &manifest::StepConfig,
) -> Result<Option<HighlightEntry>, CliError> {
    if step.highlights.is_empty() {
        return Ok(None);
    }
    let ann_cfg = &step.highlights[0];
    let sel = step.highlight_selector.as_ref().or(step.selector.as_ref());
    let bounds = if let Some(sel) = sel {
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
    Ok(Some(HighlightEntry {
        bounds,
        callout: ann_cfg.callout.clone(),
        position: ann_cfg.position.clone(),
        arrow: ann_cfg.arrow,
        color: ann_cfg.color.clone(),
        border_width: None,
        icon: ann_cfg.icon.clone(),
        shape: None,
        is_click_target: None,
        callout_offset: None,
        user_edited: None,
        callout_style: None,
        button_text: None,
        spotlight: None,
        animation: Some("zoom-in".to_string()),
        delay: Some(150),
        duration: Some(450),
    }))
}

async fn resolve_blur_regions(
    browser: &Browser,
    step: &StepConfig,
) -> Result<Vec<ElementBounds>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.blur_regions {
        if let Some(bounds) = browser.get_bounds(&cfg.selector).await? {
            results.push(bounds);
        }
    }
    Ok(results)
}

async fn resolve_arrows(
    browser: &Browser,
    step: &StepConfig,
) -> Result<Vec<ArrowPointer>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.arrows {
        let from = browser.get_element_center(&cfg.from_selector).await?;
        let to = browser.get_element_center(&cfg.to_selector).await?;
        if let (Some(from), Some(to)) = (from, to) {
            results.push(ArrowPointer {
                from,
                to,
                color: cfg.color.clone(),
                stroke_width: cfg.stroke_width,
                curvature: cfg.curvature,
                text: None,
            });
        }
    }
    Ok(results)
}

async fn resolve_hotspots(
    browser: &Browser,
    step: &StepConfig,
) -> Result<Vec<HotspotIndicator>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.hotspots {
        if let Some(center) = browser.get_element_center(&cfg.selector).await? {
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
) -> Result<Vec<PopupIndicator>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.popups {
        if let Some(center) = browser.get_element_center(&cfg.selector).await? {
            results.push(PopupIndicator {
                x: center.x,
                y: center.y,
                title: cfg.title.clone(),
                body: cfg.body.clone(),
                width: cfg.width,
                color: cfg.color.clone(),
                text_color: cfg.text_color.clone(),
                border_radius: None,
                animation: Some("fade-up".to_string()),
                delay: Some(150),
                duration: Some(450),
                dismissible: None,
                is_click_target: None,
                button_text: None,
                button_url: None,
            });
        }
    }
    Ok(results)
}

async fn resolve_ctas(browser: &Browser, step: &StepConfig) -> Result<Vec<CtaButton>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.ctas {
        let (x, y) = if let Some(ref sel) = cfg.selector {
            if let Some(center) = browser.get_element_center(sel).await? {
                (center.x, center.y)
            } else {
                continue;
            }
        } else if let (Some(x), Some(y)) = (cfg.x, cfg.y) {
            (x, y)
        } else {
            continue;
        };
        results.push(CtaButton {
            x,
            y,
            label: cfg.label.clone(),
            url: cfg.url.clone(),
            open_in_new_tab: cfg.open_in_new_tab,
            variant: cfg.variant.clone(),
            size: cfg.size.clone(),
            color: cfg.color.clone(),
            text_color: cfg.text_color.clone(),
            border_radius: None,
            animation: Some("fade-up".to_string()),
            delay: Some(150),
            duration: Some(450),
            is_click_target: None,
        });
    }
    Ok(results)
}

async fn resolve_zoom_regions(
    browser: &Browser,
    step: &StepConfig,
) -> Result<Vec<ZoomRegion>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.zoom_regions {
        if let Some(bounds) = browser.get_bounds(&cfg.selector).await? {
            results.push(ZoomRegion {
                bounds,
                magnification: cfg.magnification,
                delay: cfg.delay,
                duration: cfg.duration,
            });
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
