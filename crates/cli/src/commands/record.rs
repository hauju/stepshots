use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};
use manifest::{
    ArrowPointer, BundleManifest, BundleManifestStep, ElementBounds, HighlightEntry,
    HotspotIndicator, Point2D, PopupIndicator, StepConfig, Viewport, ZoomRegion, resolve_viewport,
};

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::bundler::create_bundle;
use crate::config::{StepshotsConfig, TutorialConfig};
use crate::error::CliError;
use crate::output::{AnnotationWarning, RecordOutput, StepOutput, TutorialOutput};

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
    // Annotations whose anchors drifted (element gone or off-screen) during recording.
    let mut drift: Vec<AnnotationDrift> = Vec::new();
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

        let drift_start = drift.len();

        wait_for_step_target(&browser, step).await?;
        let capture_before_action = should_capture_before_action(step);
        restore_scene_scroll(&browser, step).await?;

        // For click/navigate we capture the scene (and resolve overlays) BEFORE the
        // action fires; for everything else we capture AFTER it settles.
        let mut scene_url = None;
        let mut overlays = ResolvedOverlays::default();
        if capture_before_action {
            scene_url = get_current_url(&browser).await;
            overlays = resolve_overlays(&browser, step, viewport, i + 1, &mut drift).await?;
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

        if !capture_before_action {
            scene_url = get_current_url(&browser).await;
            overlays = resolve_overlays(&browser, step, viewport, i + 1, &mut drift).await?;
            let png = browser.screenshot().await?;
            screenshots.push(png);
        }

        let ResolvedOverlays {
            highlight: step_highlight,
            blurs: step_blurs,
            arrows: step_arrows,
            hotspots: step_hotspots,
            popups: step_popups,
            zooms: step_zooms,
        } = overlays;

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

        let step_warnings: Vec<AnnotationWarning> =
            drift[drift_start..].iter().map(AnnotationDrift::to_output).collect();
        step_results.push(StepOutput {
            index: i,
            name: step.name.clone(),
            action: step.action.clone(),
            selector: step.selector.clone(),
            status: if step_warnings.is_empty() { "ok" } else { "drift" },
            error: None,
            annotation_warnings: if step_warnings.is_empty() {
                None
            } else {
                Some(step_warnings)
            },
        });

        pb.inc(1);
    }

    pb.finish_with_message("done");

    if !json && !drift.is_empty() {
        eprintln!(
            "  \u{26a0} {} annotation(s) drifted during recording:",
            drift.len()
        );
        for d in &drift {
            eprintln!(
                "    - Step {} \"{}\": {} target \"{}\" {}",
                d.step_num,
                d.step_name,
                d.kind,
                d.selector,
                d.reason.describe()
            );
        }
    }

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

/// Why an annotation could not be anchored onto a recorded step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DriftReason {
    /// The selector matched nothing — the element no longer exists on the page.
    Orphaned,
    /// The element was found but resolved outside the captured viewport.
    OffScreen,
}

impl DriftReason {
    fn as_str(self) -> &'static str {
        match self {
            DriftReason::Orphaned => "orphaned",
            DriftReason::OffScreen => "off_screen",
        }
    }

    fn describe(self) -> &'static str {
        match self {
            DriftReason::Orphaned => "no longer exists (element not found)",
            DriftReason::OffScreen => "resolved off-screen",
        }
    }
}

/// A single annotation whose anchor drifted during recording.
struct AnnotationDrift {
    step_num: usize,
    step_name: String,
    kind: &'static str,
    selector: String,
    reason: DriftReason,
}

impl AnnotationDrift {
    fn to_output(&self) -> AnnotationWarning {
        AnnotationWarning {
            kind: self.kind,
            selector: self.selector.clone(),
            reason: self.reason.as_str(),
        }
    }
}

/// Classify resolved bounds: `None` element (selector matched nothing) → orphaned;
/// found but off-viewport → off-screen; visible → `None` (renders fine).
fn classify_bounds(bounds: Option<&ElementBounds>, viewport: &Viewport) -> Option<DriftReason> {
    match bounds {
        None => Some(DriftReason::Orphaned),
        Some(b) if is_bounds_visible(b, viewport) => None,
        Some(_) => Some(DriftReason::OffScreen),
    }
}

/// Point equivalent of [`classify_bounds`].
fn classify_point(point: Option<&Point2D>, viewport: &Viewport) -> Option<DriftReason> {
    match point {
        None => Some(DriftReason::Orphaned),
        Some(p) if is_point_visible(p.x, p.y, viewport) => None,
        Some(_) => Some(DriftReason::OffScreen),
    }
}

fn step_name(step: &StepConfig) -> String {
    step.name.as_deref().unwrap_or("").to_string()
}

/// All overlays resolved for a single step's scene.
#[derive(Default)]
struct ResolvedOverlays {
    highlight: Option<HighlightEntry>,
    blurs: Vec<ElementBounds>,
    arrows: Vec<ArrowPointer>,
    hotspots: Vec<HotspotIndicator>,
    popups: Vec<PopupIndicator>,
    zooms: Vec<ZoomRegion>,
}

/// Resolve every annotation kind for a step, recording any drift into `drift`.
async fn resolve_overlays(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<ResolvedOverlays, CliError> {
    let highlight = resolve_highlight(browser, step, viewport, step_num, drift).await?;
    let blurs = resolve_blur_regions(browser, step, viewport, step_num, drift).await?;
    let arrows = resolve_arrows(browser, step, viewport, step_num, drift).await?;
    let hotspots = resolve_hotspots(browser, step, viewport, step_num, drift).await?;
    let popups = resolve_popups(browser, step, viewport, step_num, drift).await?;
    let zooms = resolve_zoom_regions(browser, step, viewport, step_num, drift).await?;
    Ok(ResolvedOverlays {
        highlight,
        blurs,
        arrows,
        hotspots,
        popups,
        zooms,
    })
}

/// Resolve highlight config into a HighlightEntry with element bounds.
async fn resolve_highlight(
    browser: &Browser,
    step: &manifest::StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<Option<HighlightEntry>, CliError> {
    if step.highlights.is_empty() {
        return Ok(None);
    }
    let ann_cfg = &step.highlights[0];
    let explicit_bounds = ann_cfg.bounds.clone();
    let sel = step.highlight_selector.as_ref().or(step.selector.as_ref());
    // Explicit pixel bounds win; otherwise resolve the selector live (None == element gone).
    let bounds = if explicit_bounds.is_some() {
        explicit_bounds
    } else if let Some(sel) = sel {
        browser.get_bounds(sel).await?
    } else {
        None
    };
    if let Some(reason) = classify_bounds(bounds.as_ref(), viewport) {
        drift.push(AnnotationDrift {
            step_num,
            step_name: step_name(step),
            kind: "highlight",
            selector: sel.map(|s| s.to_string()).unwrap_or_else(|| "(none)".to_string()),
            reason,
        });
        return Ok(None);
    }
    let bounds = bounds.unwrap_or(manifest::ElementBounds {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        z_index: None,
    });
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
        z_index: None,
    }))
}

async fn resolve_blur_regions(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<Vec<ElementBounds>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.blur_regions {
        let bounds = browser.get_bounds(&cfg.selector).await?;
        match classify_bounds(bounds.as_ref(), viewport) {
            None => {
                if let Some(bounds) = bounds {
                    results.push(bounds);
                }
            }
            Some(reason) => drift.push(AnnotationDrift {
                step_num,
                step_name: step_name(step),
                kind: "blur",
                selector: cfg.selector.clone(),
                reason,
            }),
        }
    }
    Ok(results)
}

async fn resolve_arrows(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<Vec<ArrowPointer>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.arrows {
        let from = browser.get_element_center(&cfg.from_selector).await?;
        let to = browser.get_element_center(&cfg.to_selector).await?;
        let from_drift = classify_point(from.as_ref(), viewport);
        let to_drift = classify_point(to.as_ref(), viewport);
        match (from, to) {
            (Some(from), Some(to)) if from_drift.is_none() && to_drift.is_none() => {
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
                    z_index: None,
                });
            }
            _ => {
                // Record drift for each endpoint that failed to anchor.
                for (selector, reason) in [
                    (&cfg.from_selector, from_drift),
                    (&cfg.to_selector, to_drift),
                ] {
                    if let Some(reason) = reason {
                        drift.push(AnnotationDrift {
                            step_num,
                            step_name: step_name(step),
                            kind: "arrow",
                            selector: selector.clone(),
                            reason,
                        });
                    }
                }
            }
        }
    }
    Ok(results)
}

async fn resolve_hotspots(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<Vec<HotspotIndicator>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.hotspots {
        let center = browser.get_element_center(&cfg.selector).await?;
        match classify_point(center.as_ref(), viewport) {
            None => {
                if let Some(center) = center {
                    results.push(HotspotIndicator {
                        x: center.x,
                        y: center.y,
                        color: cfg.color.clone(),
                        size: cfg.size,
                        callout: cfg.callout.clone(),
                        position: cfg.position.clone(),
                        is_click_target: cfg.is_click_target,
                        z_index: None,
                    });
                }
            }
            Some(reason) => drift.push(AnnotationDrift {
                step_num,
                step_name: step_name(step),
                kind: "hotspot",
                selector: cfg.selector.clone(),
                reason,
            }),
        }
    }
    Ok(results)
}

async fn resolve_popups(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<Vec<PopupIndicator>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.popups {
        let center = browser.get_element_center(&cfg.selector).await?;
        let center = match classify_point(center.as_ref(), viewport) {
            None => match center {
                Some(center) => center,
                None => continue,
            },
            Some(reason) => {
                drift.push(AnnotationDrift {
                    step_num,
                    step_name: step_name(step),
                    kind: "popup",
                    selector: cfg.selector.clone(),
                    reason,
                });
                continue;
            }
        };
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
            z_index: None,
        });
    }
    Ok(results)
}

async fn resolve_zoom_regions(
    browser: &Browser,
    step: &StepConfig,
    viewport: &Viewport,
    step_num: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<Vec<ZoomRegion>, CliError> {
    let mut results = Vec::new();
    for cfg in &step.zoom_regions {
        let bounds = browser.get_bounds(&cfg.selector).await?;
        match classify_bounds(bounds.as_ref(), viewport) {
            None => {
                if let Some(bounds) = bounds {
                    results.push(ZoomRegion {
                        bounds,
                        magnification: cfg.magnification,
                        delay: cfg.delay,
                        duration: cfg.duration,
                        z_index: None,
                    });
                }
            }
            Some(reason) => drift.push(AnnotationDrift {
                step_num,
                step_name: step_name(step),
                kind: "zoom",
                selector: cfg.selector.clone(),
                reason,
            }),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Viewport {
        Viewport {
            width: 1280,
            height: 800,
            device_scale_factor: None,
        }
    }

    fn bounds(x: f64, y: f64, w: f64, h: f64) -> ElementBounds {
        ElementBounds {
            x,
            y,
            width: w,
            height: h,
            z_index: None,
        }
    }

    #[test]
    fn missing_element_is_orphaned() {
        // A selector that resolved to nothing (None) means the element is gone.
        assert_eq!(classify_bounds(None, &viewport()), Some(DriftReason::Orphaned));
        assert_eq!(classify_point(None, &viewport()), Some(DriftReason::Orphaned));
    }

    #[test]
    fn visible_element_does_not_drift() {
        let b = bounds(100.0, 100.0, 50.0, 20.0);
        assert_eq!(classify_bounds(Some(&b), &viewport()), None);
        let p = Point2D { x: 100.0, y: 100.0 };
        assert_eq!(classify_point(Some(&p), &viewport()), None);
    }

    #[test]
    fn found_but_off_viewport_is_off_screen() {
        // Element exists but resolved past the right/bottom edge.
        let b = bounds(2000.0, 100.0, 50.0, 20.0);
        assert_eq!(
            classify_bounds(Some(&b), &viewport()),
            Some(DriftReason::OffScreen)
        );
        let p = Point2D { x: 100.0, y: 2000.0 };
        assert_eq!(
            classify_point(Some(&p), &viewport()),
            Some(DriftReason::OffScreen)
        );
    }

    #[test]
    fn reason_serializes_to_stable_strings() {
        assert_eq!(DriftReason::Orphaned.as_str(), "orphaned");
        assert_eq!(DriftReason::OffScreen.as_str(), "off_screen");
    }
}
