use std::path::Path;

use indicatif::{ProgressBar, ProgressStyle};
use manifest::{StepConfig, Viewport, resolve_viewport};

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::config::{StepshotsConfig, TutorialConfig, select_tutorials};
use crate::error::CliError;
use crate::output::{
    VerifyAnnotationWarning, VerifyFailure, VerifyOutput, VerifyStep, VerifySummary, VerifyTutorial,
};

use super::record::{
    AnnotationDrift, get_current_url, resolve_overlays, resolve_url, restore_scene_scroll,
    save_debug_screenshot, should_capture_before_action, wait_for_step_target,
};

/// Which drift level makes `verify` exit non-zero.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FailOn {
    /// Only steps that can no longer execute fail the run.
    Fail,
    /// Annotation drift (warn-level) also fails the run.
    Warn,
}

/// Verify that tutorials still replay against the live app. Replays every
/// step headless like `record` does, but writes no bundle — the output is
/// a per-step drift report and an exit code.
pub async fn run(
    config: &StepshotsConfig,
    config_path: &Path,
    tutorials: &[String],
    save_failures: &Path,
    fail_on: FailOn,
    json: bool,
    session: crate::browser::SessionArgs<'_>,
) -> Result<(), CliError> {
    let selected = select_tutorials(config, tutorials)?;
    let viewport = resolve_viewport(config.format.as_ref(), &config.viewport);
    // Loading up front keeps a bad session file from surfacing as every
    // authenticated step "drifting" because the run was silently logged out.
    let storage_state = session.load(!json)?;

    if !json {
        println!(
            "Verifying {} tutorial(s) against {}\n",
            selected.len(),
            config.base_url
        );
    }

    let mut results: Vec<VerifyTutorial> = Vec::with_capacity(selected.len());
    for (key, tutorial) in &selected {
        let result = verify_tutorial(
            config,
            tutorial,
            &viewport,
            key,
            save_failures,
            json,
            session.with_state(storage_state.as_ref()),
        )
        .await?;
        if !json {
            print_tutorial_result(&result);
        }
        results.push(result);
    }

    let summary = summarize(&results);
    let should_fail = exits_nonzero(&summary, fail_on);

    if json {
        let out = VerifyOutput {
            success: !should_fail,
            command: "verify",
            config: config_path.display().to_string(),
            summary,
            tutorials: results,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing VerifyOutput")
        );
    } else {
        println!(
            "\n{} fresh · {} warning(s) · {} broken",
            summary.ok, summary.warn, summary.fail
        );
    }

    if should_fail {
        return Err(CliError::Reported { code: 1 });
    }
    Ok(())
}

/// Verify a single tutorial. Only browser-level problems (launch failure)
/// return `Err`; a start page that no longer loads is itself drift and is
/// reported in the returned [`VerifyTutorial`].
async fn verify_tutorial(
    config: &StepshotsConfig,
    tutorial: &TutorialConfig,
    viewport: &Viewport,
    key: &str,
    save_failures: &Path,
    json: bool,
    session: crate::browser::SessionSource<'_>,
) -> Result<VerifyTutorial, CliError> {
    // Seeds cookies before the first navigation — cookies set after a page
    // loads arrive too late to authenticate it.
    let browser = Browser::launch_with_session(viewport, true, session).await?;

    if let Some(ref theme) = config.theme {
        browser.set_color_scheme(theme).await?;
    }

    let start_url = resolve_url(&config.base_url, &tutorial.url);
    if let Err(e) = browser.navigate(&start_url).await {
        return Ok(VerifyTutorial {
            key: key.to_string(),
            title: tutorial.title.clone(),
            status: "fail",
            error: Some(VerifyFailure {
                reason: "navigation_failed",
                message: e.to_string(),
                page_url: Some(start_url.clone()),
                screenshot: None,
                hint: format!(
                    "Check that '{start_url}' still exists, or update `url`/`baseUrl` in stepshots.config.json."
                ),
            }),
            steps: tutorial
                .steps
                .iter()
                .enumerate()
                .map(|(i, s)| skipped_step(i, s))
                .collect(),
            annotation_warnings: Vec::new(),
        });
    }
    browser.wait_idle(config.default_delay).await;

    let pb = if json {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new(tutorial.steps.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  [{bar:30}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("=> "),
        );
        pb
    };

    let mut steps: Vec<VerifyStep> = Vec::with_capacity(tutorial.steps.len());
    let mut drift: Vec<AnnotationDrift> = Vec::new();
    let mut failed = false;

    for (i, step) in tutorial.steps.iter().enumerate() {
        if failed {
            // Page state after a failed step is undefined; checking further
            // steps would report cascading false failures.
            steps.push(skipped_step(i, step));
            continue;
        }

        pb.set_message(format!(
            "{}: {} ({key})",
            step.action,
            step.selector.as_deref().unwrap_or("")
        ));

        match check_step(&browser, config, step, viewport, i, &mut drift).await {
            Ok(()) => {
                steps.push(VerifyStep {
                    index: i,
                    name: step.name.clone(),
                    action: step.action.clone(),
                    selector: step.selector.clone(),
                    status: "ok",
                    failure: None,
                });
                pb.inc(1);
            }
            Err((reason, e)) => {
                // Capture debugging context while the failed page is still open.
                let page_url = get_current_url(&browser).await;
                let screenshot = save_debug_screenshot(
                    &browser,
                    &save_failures.join(format!("{key}.stepshot")),
                    i + 1,
                )
                .await;
                steps.push(VerifyStep {
                    index: i,
                    name: step.name.clone(),
                    action: step.action.clone(),
                    selector: step.selector.clone(),
                    status: "fail",
                    failure: Some(VerifyFailure {
                        reason,
                        message: e.to_string(),
                        page_url: page_url.clone(),
                        screenshot: screenshot.map(|p| p.display().to_string()),
                        hint: hint_for(reason, key, page_url.as_deref().unwrap_or(&start_url)),
                    }),
                });
                failed = true;
            }
        }
    }
    pb.finish_and_clear();

    Ok(VerifyTutorial {
        key: key.to_string(),
        title: tutorial.title.clone(),
        status: tutorial_status(failed, drift.len()),
        error: None,
        steps,
        annotation_warnings: drift
            .iter()
            .map(|d| VerifyAnnotationWarning {
                step: d.step_num - 1,
                kind: d.kind,
                selector: d.selector.clone(),
                reason: d.reason.as_str(),
            })
            .collect(),
    })
}

/// Run one step's checks in the same order `record` executes them,
/// classifying any failure by the kind of drift it indicates.
async fn check_step(
    browser: &Browser,
    config: &StepshotsConfig,
    step: &StepConfig,
    viewport: &Viewport,
    index: usize,
    drift: &mut Vec<AnnotationDrift>,
) -> Result<(), (&'static str, CliError)> {
    wait_for_step_target(browser, step)
        .await
        .map_err(|e| ("orphaned", e))?;
    restore_scene_scroll(browser, step)
        .await
        .map_err(|e| ("action_failed", e))?;

    let capture_before_action = should_capture_before_action(step);
    if capture_before_action {
        resolve_overlays(browser, step, viewport, index + 1, drift)
            .await
            .map_err(|e| ("action_failed", e))?;
    }

    let action_reason = match step.action.as_str() {
        "navigate" => "navigation_failed",
        // A `wait` step that times out means the awaited element never appeared.
        "wait" if step.selector.is_some() => "orphaned",
        _ => "action_failed",
    };
    let action_result = execute_action(browser, step, &config.base_url)
        .await
        .map_err(|e| (action_reason, e))?;

    if action_result.transition_frames.is_empty() {
        let delay = step.delay.unwrap_or(config.default_delay);
        browser.wait_idle(delay).await;
    }
    if !capture_before_action {
        resolve_overlays(browser, step, viewport, index + 1, drift)
            .await
            .map_err(|e| ("action_failed", e))?;
    }
    Ok(())
}

fn skipped_step(index: usize, step: &StepConfig) -> VerifyStep {
    VerifyStep {
        index,
        name: step.name.clone(),
        action: step.action.clone(),
        selector: step.selector.clone(),
        status: "skipped",
        failure: None,
    }
}

fn tutorial_status(has_failure: bool, drift_count: usize) -> &'static str {
    if has_failure {
        "fail"
    } else if drift_count > 0 {
        "warn"
    } else {
        "ok"
    }
}

fn summarize(tutorials: &[VerifyTutorial]) -> VerifySummary {
    VerifySummary {
        tutorials: tutorials.len(),
        ok: tutorials.iter().filter(|t| t.status == "ok").count(),
        warn: tutorials.iter().filter(|t| t.status == "warn").count(),
        fail: tutorials.iter().filter(|t| t.status == "fail").count(),
        steps_checked: tutorials
            .iter()
            .flat_map(|t| &t.steps)
            .filter(|s| s.status != "skipped")
            .count(),
    }
}

fn exits_nonzero(summary: &VerifySummary, fail_on: FailOn) -> bool {
    summary.fail > 0 || (fail_on == FailOn::Warn && summary.warn > 0)
}

/// Suggested next command for an agent (or human) to repair the drift.
fn hint_for(reason: &str, key: &str, page_url: &str) -> String {
    match reason {
        "orphaned" => format!(
            "Run `stepshots inspect {page_url}` to find a replacement selector, then update this step in stepshots.config.json."
        ),
        "navigation_failed" => {
            "Check that the target URL still exists, or update `url`/`baseUrl` in stepshots.config.json.".to_string()
        }
        _ => format!("Run `stepshots preview {key}` to watch this flow in a visible browser."),
    }
}

fn print_tutorial_result(result: &VerifyTutorial) {
    let key = &result.key;
    let total = result.steps.len();
    match result.status {
        "ok" => println!("✓ {key} — {total}/{total} steps ok"),
        "warn" => {
            println!(
                "⚠ {key} — {total}/{total} steps ok, {} annotation(s) drifted",
                result.annotation_warnings.len()
            );
            for w in &result.annotation_warnings {
                let why = match w.reason {
                    "off_screen" => "resolved off-screen",
                    _ => "no longer exists (element not found)",
                };
                println!(
                    "    step {}: {} '{}' {}",
                    w.step + 1,
                    w.kind,
                    w.selector,
                    why
                );
            }
        }
        _ => {
            if let Some(err) = &result.error {
                println!("✗ {key} — start page failed to load");
                println!("    {}", err.message);
                println!("    hint: {}", err.hint);
            } else if let Some(fs) = result.steps.iter().find(|s| s.status == "fail") {
                println!("✗ {key} — failed at step {}/{}", fs.index + 1, total);
                let target = fs
                    .selector
                    .as_deref()
                    .map(|s| format!(" '{s}'"))
                    .unwrap_or_default();
                let f = fs.failure.as_ref().expect("failed step carries a failure");
                println!("    {}{target} — {}", fs.action, f.message);
                if let Some(shot) = &f.screenshot {
                    println!("    screenshot: {shot}");
                }
                println!("    hint: {}", f.hint);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(ok: usize, warn: usize, fail: usize) -> VerifySummary {
        VerifySummary {
            tutorials: ok + warn + fail,
            ok,
            warn,
            fail,
            steps_checked: 0,
        }
    }

    #[test]
    fn status_prefers_fail_over_warn() {
        assert_eq!(tutorial_status(true, 3), "fail");
        assert_eq!(tutorial_status(false, 3), "warn");
        assert_eq!(tutorial_status(false, 0), "ok");
    }

    #[test]
    fn fail_on_fail_ignores_warnings() {
        assert!(!exits_nonzero(&summary(1, 2, 0), FailOn::Fail));
        assert!(exits_nonzero(&summary(1, 0, 1), FailOn::Fail));
    }

    #[test]
    fn fail_on_warn_also_fails_on_drift() {
        assert!(exits_nonzero(&summary(1, 2, 0), FailOn::Warn));
        assert!(!exits_nonzero(&summary(3, 0, 0), FailOn::Warn));
    }

    #[test]
    fn hints_point_at_repair_commands() {
        assert!(
            hint_for("orphaned", "demo", "https://x.test/settings")
                .contains("stepshots inspect https://x.test/settings")
        );
        assert!(
            hint_for("action_failed", "demo", "https://x.test").contains("stepshots preview demo")
        );
    }
}
