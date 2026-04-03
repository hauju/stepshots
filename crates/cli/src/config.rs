use std::collections::HashMap;
use std::path::{Path, PathBuf};

// Re-export config types from the shared manifest crate.
#[allow(unused_imports)]
pub use manifest::{
    ArrowConfig, BlurConfig, CtaConfig, DemoFormat, HighlightConfig, HotspotConfig, PopupConfig,
    StepConfig, StepshotsConfig, TutorialConfig, ZoomConfig, default_delay, default_viewport,
};

use crate::error::CliError;

/// Find the config file by searching:
/// 1. Explicit `--config` path
/// 2. `STEPSHOTS_CONFIG` env var
/// 3. Walk up from CWD looking for `stepshots.config.json`
pub fn find_config(explicit: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(CliError::Config(format!(
            "Config file not found: {}",
            path.display()
        )));
    }

    if let Ok(env_path) = std::env::var("STEPSHOTS_CONFIG") {
        let p = PathBuf::from(&env_path);
        if p.exists() {
            return Ok(p);
        }
        return Err(CliError::Config(format!(
            "STEPSHOTS_CONFIG points to missing file: {env_path}"
        )));
    }

    let mut dir = std::env::current_dir().map_err(|e| CliError::Config(e.to_string()))?;
    loop {
        let candidate = dir.join("stepshots.config.json");
        if candidate.exists() {
            return Ok(candidate);
        }
        if !dir.pop() {
            break;
        }
    }

    Err(CliError::Config(
        "No stepshots.config.json found. Run `stepshots init` to create one.".into(),
    ))
}

/// Replace `${VAR_NAME}` patterns with their environment variable values.
/// Variables that are not set in the environment are left as-is.
fn substitute_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.char_indices().peekable();

    while let Some((_, ch)) = chars.next() {
        if ch == '$' {
            if let Some(&(_, '{')) = chars.peek() {
                chars.next(); // consume '{'
                let start = if let Some(&(pos, _)) = chars.peek() {
                    pos
                } else {
                    result.push_str("${");
                    break;
                };
                let mut end = start;
                let mut found_close = false;
                while let Some(&(pos, c)) = chars.peek() {
                    if c == '}' {
                        end = pos;
                        found_close = true;
                        chars.next(); // consume '}'
                        break;
                    }
                    end = pos + c.len_utf8();
                    chars.next();
                }
                if found_close {
                    let var_name = &input[start..end];
                    if let Ok(val) = std::env::var(var_name) {
                        result.push_str(&val);
                    } else {
                        result.push_str("${");
                        result.push_str(var_name);
                        result.push('}');
                    }
                } else {
                    result.push_str("${");
                    result.push_str(&input[start..end]);
                }
            } else {
                result.push(ch);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Load and parse the config file.
/// Supports `${VAR}` environment variable interpolation in all string values.
pub fn load_config(path: &Path) -> Result<StepshotsConfig, CliError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| CliError::Config(format!("Failed to read {}: {e}", path.display())))?;
    let content = substitute_env_vars(&content);
    let config: StepshotsConfig = serde_json::from_str(&content)?;

    if config.tutorials.is_empty() {
        return Err(CliError::Config("Config has no tutorials defined".into()));
    }

    let errors = validate_config(&config);
    if !errors.is_empty() {
        let mut msg = String::from("Config validation failed:");
        for e in &errors {
            msg.push_str(&format!("\n  {}: {}", e.path, e.message));
        }
        return Err(CliError::Config(msg));
    }

    Ok(config)
}

// ---------------------------------------------------------------------------
// Config validation
// ---------------------------------------------------------------------------

const VALID_ACTIONS: &[&str] = &[
    "click",
    "type",
    "key",
    "scroll",
    "scroll-to",
    "hover",
    "navigate",
    "wait",
    "select",
];

const VALID_POSITIONS: &[&str] = &["top", "bottom", "left", "right"];

struct ConfigError {
    path: String,
    message: String,
}

fn validate_config(config: &StepshotsConfig) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    for (key, tutorial) in &config.tutorials {
        errors.extend(validate_tutorial(key, tutorial));
    }
    errors
}

fn validate_tutorial(key: &str, tutorial: &TutorialConfig) -> Vec<ConfigError> {
    let mut errors = Vec::new();
    let base = format!("tutorials.{key}");

    if tutorial.url.is_empty() {
        errors.push(ConfigError {
            path: base.clone(),
            message: "\"url\" must not be empty".into(),
        });
    }
    if tutorial.steps.is_empty() {
        errors.push(ConfigError {
            path: base.clone(),
            message: "\"steps\" must not be empty".into(),
        });
    }

    for (i, step) in tutorial.steps.iter().enumerate() {
        errors.extend(validate_step(&format!("{base}.steps[{i}]"), step));
    }
    errors
}

fn validate_step(path: &str, step: &StepConfig) -> Vec<ConfigError> {
    let mut errors = Vec::new();

    // Check action is valid
    if !VALID_ACTIONS.contains(&step.action.as_str()) {
        errors.push(ConfigError {
            path: path.into(),
            message: format!(
                "unknown action \"{}\" (valid: {})",
                step.action,
                VALID_ACTIONS.join(", ")
            ),
        });
        return errors; // skip field checks for unknown actions
    }

    // Action-specific required fields
    match step.action.as_str() {
        "click" | "hover" => {
            if step.selector.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: format!("\"{}\" action requires \"selector\"", step.action),
                });
            }
        }
        "type" => {
            if step.selector.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: "\"type\" action requires \"selector\"".into(),
                });
            }
            if step.text.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: "\"type\" action requires \"text\"".into(),
                });
            }
        }
        "key" => {
            if step.key.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: "\"key\" action requires \"key\"".into(),
                });
            }
        }
        "scroll-to" => {
            if step.selector.is_none() && step.highlight_selector.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: "\"scroll-to\" requires \"selector\" or \"highlightSelector\"".into(),
                });
            }
        }
        "navigate" => {
            if step.url.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: "\"navigate\" action requires \"url\"".into(),
                });
            }
        }
        "select" => {
            if step.selector.is_none() {
                errors.push(ConfigError {
                    path: path.into(),
                    message: "\"select\" action requires \"selector\"".into(),
                });
            }
        }
        _ => {} // scroll, wait — no strict requirements
    }

    // Validate overlay configs
    for (i, h) in step.highlights.iter().enumerate() {
        if let Some(ref pos) = h.position
            && !VALID_POSITIONS.contains(&pos.as_str())
        {
            errors.push(ConfigError {
                path: format!("{path}.highlights[{i}]"),
                message: format!(
                    "invalid position \"{pos}\" (valid: {})",
                    VALID_POSITIONS.join(", ")
                ),
            });
        }
    }

    for (i, b) in step.blur_regions.iter().enumerate() {
        if b.selector.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.blurRegions[{i}]"),
                message: "\"selector\" must not be empty".into(),
            });
        }
    }

    for (i, a) in step.arrows.iter().enumerate() {
        if a.from_selector.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.arrows[{i}]"),
                message: "\"fromSelector\" must not be empty".into(),
            });
        }
        if a.to_selector.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.arrows[{i}]"),
                message: "\"toSelector\" must not be empty".into(),
            });
        }
    }

    for (i, h) in step.hotspots.iter().enumerate() {
        if h.selector.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.hotspots[{i}]"),
                message: "\"selector\" must not be empty".into(),
            });
        }
        if let Some(ref pos) = h.position
            && !VALID_POSITIONS.contains(&pos.as_str())
        {
            errors.push(ConfigError {
                path: format!("{path}.hotspots[{i}]"),
                message: format!(
                    "invalid position \"{pos}\" (valid: {})",
                    VALID_POSITIONS.join(", ")
                ),
            });
        }
    }

    for (i, p) in step.popups.iter().enumerate() {
        if p.selector.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.popups[{i}]"),
                message: "\"selector\" must not be empty".into(),
            });
        }
        if p.body.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.popups[{i}]"),
                message: "\"body\" must not be empty".into(),
            });
        }
    }

    for (i, c) in step.ctas.iter().enumerate() {
        if c.selector.is_none() && (c.x.is_none() || c.y.is_none()) {
            errors.push(ConfigError {
                path: format!("{path}.ctas[{i}]"),
                message: "CTA requires \"selector\" or both \"x\" and \"y\"".into(),
            });
        }
        if c.label.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.ctas[{i}]"),
                message: "\"label\" must not be empty".into(),
            });
        }
    }

    for (i, z) in step.zoom_regions.iter().enumerate() {
        if z.selector.is_empty() {
            errors.push(ConfigError {
                path: format!("{path}.zoomRegions[{i}]"),
                message: "\"selector\" must not be empty".into(),
            });
        }
    }

    errors
}

/// Generate a sample config.
pub fn sample_config() -> String {
    serde_json::to_string_pretty(&StepshotsConfig {
        base_url: "https://example.com".into(),
        viewport: default_viewport(),
        format: Some(DemoFormat::Desktop),
        default_delay: default_delay(),
        theme: None,
        tutorials: HashMap::from([(
            "getting-started".into(),
            TutorialConfig {
                url: "/".into(),
                title: "Getting Started".into(),
                description: Some("A quick tour of the product.".into()),
                steps: vec![
                    StepConfig {
                        action: "click".into(),
                        name: None,
                        selector: Some("button.cta".into()),
                        text: None,
                        url: None,
                        key: None,
                        value: None,
                        delay: None,
                        scroll_x: None,
                        scroll_y: None,
                        highlight_selector: None,
                        highlights: vec![HighlightConfig {
                            show_border: Some(true),
                            callout: Some("Click here to get started".into()),
                            icon: None,
                            position: Some("bottom".into()),
                            color: None,
                            arrow: None,
                        }],
                        blur_regions: vec![],
                        arrows: vec![],
                        hotspots: vec![],
                        popups: vec![],
                        ctas: vec![],
                        zoom_regions: vec![],
                    },
                    StepConfig {
                        action: "type".into(),
                        name: None,
                        selector: Some("input[name=\"email\"]".into()),
                        text: Some("user@example.com".into()),
                        url: None,
                        key: None,
                        value: None,
                        delay: None,
                        scroll_x: None,
                        scroll_y: None,
                        highlight_selector: None,
                        highlights: vec![],
                        blur_regions: vec![],
                        arrows: vec![],
                        hotspots: vec![],
                        popups: vec![],
                        ctas: vec![],
                        zoom_regions: vec![],
                    },
                ],
            },
        )]),
    })
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn set_var(key: &str, val: &str) {
        unsafe { std::env::set_var(key, val) };
    }

    unsafe fn remove_var(key: &str) {
        unsafe { std::env::remove_var(key) };
    }

    #[test]
    fn substitutes_set_env_var() {
        // SAFETY: test runs single-threaded
        unsafe { set_var("TEST_SUBST_A", "hello") };
        assert_eq!(substitute_env_vars("${TEST_SUBST_A}"), "hello");
        unsafe { remove_var("TEST_SUBST_A") };
    }

    #[test]
    fn leaves_unset_var_as_is() {
        // SAFETY: test runs single-threaded
        unsafe { remove_var("TEST_SUBST_MISSING") };
        assert_eq!(
            substitute_env_vars("${TEST_SUBST_MISSING}"),
            "${TEST_SUBST_MISSING}"
        );
    }

    #[test]
    fn substitutes_multiple_vars() {
        // SAFETY: test runs single-threaded
        unsafe {
            set_var("TEST_SUBST_B", "foo");
            set_var("TEST_SUBST_C", "bar");
        }
        assert_eq!(
            substitute_env_vars("${TEST_SUBST_B} and ${TEST_SUBST_C}"),
            "foo and bar"
        );
        unsafe {
            remove_var("TEST_SUBST_B");
            remove_var("TEST_SUBST_C");
        }
    }

    #[test]
    fn preserves_bare_dollar_sign() {
        assert_eq!(substitute_env_vars("$100"), "$100");
    }

    #[test]
    fn handles_unclosed_brace() {
        assert_eq!(substitute_env_vars("${UNCLOSED"), "${UNCLOSED");
    }

    #[test]
    fn substitutes_in_json_context() {
        // SAFETY: test runs single-threaded
        unsafe { set_var("TEST_SUBST_EMAIL", "ci@test.com") };
        let input = r#"{"text": "${TEST_SUBST_EMAIL}"}"#;
        let expected = r#"{"text": "ci@test.com"}"#;
        assert_eq!(substitute_env_vars(input), expected);
        unsafe { remove_var("TEST_SUBST_EMAIL") };
    }

    fn make_step(action: &str) -> StepConfig {
        StepConfig {
            action: action.into(),
            name: None,
            selector: None,
            text: None,
            url: None,
            key: None,
            value: None,
            delay: None,
            scroll_x: None,
            scroll_y: None,
            highlight_selector: None,
            highlights: vec![],
            blur_regions: vec![],
            arrows: vec![],
            hotspots: vec![],
            popups: vec![],
            ctas: vec![],
            zoom_regions: vec![],
        }
    }

    #[test]
    fn validate_unknown_action() {
        let step = make_step("clck");
        let errors = validate_step("test", &step);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unknown action"));
    }

    #[test]
    fn validate_click_missing_selector() {
        let step = make_step("click");
        let errors = validate_step("test", &step);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("requires \"selector\""));
    }

    #[test]
    fn validate_click_valid() {
        let mut step = make_step("click");
        step.selector = Some("button".into());
        let errors = validate_step("test", &step);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_type_missing_both() {
        let step = make_step("type");
        let errors = validate_step("test", &step);
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn validate_navigate_missing_url() {
        let step = make_step("navigate");
        let errors = validate_step("test", &step);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("requires \"url\""));
    }

    #[test]
    fn validate_invalid_position() {
        let mut step = make_step("scroll");
        step.highlights.push(HighlightConfig {
            show_border: None,
            callout: None,
            icon: None,
            position: Some("center".into()),
            color: None,
            arrow: None,
        });
        let errors = validate_step("test", &step);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("invalid position"));
    }

    #[test]
    fn validate_scroll_no_errors() {
        let step = make_step("scroll");
        let errors = validate_step("test", &step);
        assert!(errors.is_empty());
    }

    #[test]
    fn validate_empty_tutorial() {
        let tutorial = TutorialConfig {
            url: "".into(),
            title: "Test".into(),
            description: None,
            steps: vec![],
        };
        let errors = validate_tutorial("test", &tutorial);
        assert_eq!(errors.len(), 2); // empty url + empty steps
    }
}
