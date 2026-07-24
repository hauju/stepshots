use std::path::Path;

use manifest::{DemoFormat, Viewport};

use crate::browser::Browser;
use crate::error::CliError;

/// Open a visible browser on a persistent profile so the user can log in to
/// sites that authenticated recordings need. The session (cookies, local
/// storage) is saved in the profile directory and reused by `record`,
/// `preview`, and `inspect` when they are given the same `--profile-dir`.
pub async fn run(url: &str, profile_dir: &Path, viewport: &Viewport) -> Result<(), CliError> {
    println!("Opening browser with profile: {}", profile_dir.display());
    println!("Log in to the sites you want to record, then press Ctrl+C here when done.");

    let browser = Browser::launch(viewport, false, Some(profile_dir)).await?;
    browser.navigate(url).await?;

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| CliError::Other(format!("Signal error: {e}")))?;

    println!("Profile saved: {}", profile_dir.display());
    Ok(())
}

/// Parse a `--viewport` value: either `WxH` (e.g. "1440x900") or a kebab-case
/// `DemoFormat` preset name (e.g. "mobile").
pub fn parse_viewport(s: &str) -> Result<Viewport, String> {
    if let Some((w, h)) = s.split_once('x')
        && let (Ok(width), Ok(height)) = (w.parse::<u32>(), h.parse::<u32>())
    {
        if width == 0 || height == 0 {
            return Err(format!("viewport dimensions must be non-zero: {s}"));
        }
        return Ok(Viewport {
            width,
            height,
            device_scale_factor: None,
        });
    }

    let preset: DemoFormat = serde_json::from_value(serde_json::Value::String(s.to_string()))
        .map_err(|_| {
            format!(
                "expected WxH (e.g. 1440x900) or one of: {}",
                DemoFormat::all_presets()
                    .iter()
                    .map(|f| serde_json::to_value(f)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let (width, height) = preset
        .dimensions()
        .ok_or_else(|| format!("preset '{s}' has no fixed dimensions"))?;
    Ok(Viewport {
        width,
        height,
        device_scale_factor: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_dimensions() {
        let vp = parse_viewport("1440x900").unwrap();
        assert_eq!((vp.width, vp.height), (1440, 900));
        assert_eq!(vp.device_scale_factor, None);
    }

    #[test]
    fn parses_presets() {
        let vp = parse_viewport("desktop-hd").unwrap();
        assert_eq!((vp.width, vp.height), (1920, 1080));
        let vp = parse_viewport("mobile").unwrap();
        assert_eq!((vp.width, vp.height), (390, 844));
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(parse_viewport("custom").is_err());
        assert!(parse_viewport("abc").is_err());
        assert!(parse_viewport("0x100").is_err());
        let msg = parse_viewport("nope").unwrap_err();
        assert!(msg.contains("desktop-hd"), "unexpected message: {msg}");
    }
}
