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

    Ok(config)
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
}
