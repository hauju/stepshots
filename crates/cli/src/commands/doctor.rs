use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::auth;
use crate::config;
use crate::error::CliError;
use crate::output::{DoctorCheck, DoctorOutput};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Check the environment: browser, config, server, and login.
/// Exits non-zero if any check fails (warnings don't fail the run).
pub async fn run(server: &str, explicit_config: Option<&Path>, json: bool) -> Result<(), CliError> {
    let checks = vec![
        check_browser(),
        check_config(explicit_config),
        check_server(server).await,
        check_auth(server).await,
    ];

    let failed = checks.iter().any(|c| c.status == "fail");

    if json {
        let out = DoctorOutput {
            success: !failed,
            command: "doctor",
            version: VERSION,
            checks,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing DoctorOutput")
        );
    } else {
        println!("stepshots {VERSION}");
        println!();
        for c in &checks {
            let symbol = match c.status {
                "ok" => "✓",
                "warn" => "⚠",
                _ => "✗",
            };
            println!("{symbol} {}: {}", c.name, c.detail);
            if let Some(hint) = &c.hint {
                println!("    {hint}");
            }
        }
        println!();
        if failed {
            println!("Some checks failed — fix the ✗ items above.");
        } else {
            println!("Everything looks good.");
        }
    }

    if failed {
        // Details are already printed (or in the JSON payload).
        return Err(CliError::Reported { code: 1 });
    }
    Ok(())
}

fn ok(name: &'static str, detail: String) -> DoctorCheck {
    DoctorCheck {
        name,
        status: "ok",
        detail,
        hint: None,
    }
}

fn warn(name: &'static str, detail: String, hint: &str) -> DoctorCheck {
    DoctorCheck {
        name,
        status: "warn",
        detail,
        hint: (!hint.is_empty()).then(|| hint.to_string()),
    }
}

fn fail(name: &'static str, detail: String, hint: &str) -> DoctorCheck {
    DoctorCheck {
        name,
        status: "fail",
        detail,
        hint: (!hint.is_empty()).then(|| hint.to_string()),
    }
}

/// Chrome/Chromium: CHROME_PATH wins (same as recording), otherwise
/// chromiumoxide's auto-detection — the exact lookup `record` will use.
fn check_browser() -> DoctorCheck {
    let found: Option<(PathBuf, &str)> = match std::env::var("CHROME_PATH") {
        Ok(p) if Path::new(&p).exists() => Some((PathBuf::from(p), "CHROME_PATH")),
        Ok(p) => {
            return fail(
                "browser",
                format!("CHROME_PATH points to a missing file: {p}"),
                "Fix or unset CHROME_PATH so auto-detection can run.",
            );
        }
        Err(_) => chromiumoxide::detection::default_executable(
            chromiumoxide::detection::DetectionOptions::default(),
        )
        .ok()
        .map(|p| (p, "auto-detected")),
    };

    match found {
        Some((path, source)) => {
            let version = std::process::Command::new(&path)
                .arg("--version")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|| "unknown version".to_string());
            ok(
                "browser",
                format!("{version} ({}, {source})", path.display()),
            )
        }
        None => fail(
            "browser",
            "no Chrome or Chromium found".to_string(),
            "Install Google Chrome, or set CHROME_PATH to a Chromium-based browser.",
        ),
    }
}

fn check_config(explicit: Option<&Path>) -> DoctorCheck {
    match config::find_config(explicit) {
        Err(_) => warn(
            "config",
            "no stepshots.config.json found".to_string(),
            "Run `stepshots init` to create one (fine if you're not in a project directory).",
        ),
        Ok(path) => match config::load_config(&path) {
            Ok(cfg) => ok(
                "config",
                format!("{} ({} tutorial(s))", path.display(), cfg.tutorials.len()),
            ),
            Err(e) => fail(
                "config",
                format!("{} is invalid: {e}", path.display()),
                "Fix the errors above; your editor validates the file via its \"$schema\" entry.",
            ),
        },
    }
}

async fn check_server(server: &str) -> DoctorCheck {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => return warn("server", format!("could not build HTTP client: {e}"), ""),
    };
    match client.get(server).send().await {
        Ok(resp) => ok(
            "server",
            format!("{server} reachable (HTTP {})", resp.status().as_u16()),
        ),
        Err(e) => warn(
            "server",
            format!("{server} unreachable: {e}"),
            "Recording works offline; only login and upload need the server.",
        ),
    }
}

async fn check_auth(server: &str) -> DoctorCheck {
    let server = server.trim_end_matches('/');
    let (token, source) = match std::env::var("STEPSHOTS_TOKEN") {
        Ok(t) if !t.is_empty() => (t, "STEPSHOTS_TOKEN"),
        _ => match auth::stored_token_for(server) {
            Some(t) => (t, "stored login"),
            None => {
                return warn(
                    "auth",
                    format!("not logged in to {server}"),
                    "Run `stepshots login` (or set STEPSHOTS_TOKEN) before uploading.",
                );
            }
        },
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => return warn("auth", format!("could not build HTTP client: {e}"), ""),
    };
    let resp = client
        .get(format!("{server}/api/whoami"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await;

    match resp {
        Ok(resp) if resp.status().is_success() => {
            let email = resp
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|v| v.get("email").and_then(|e| e.as_str()).map(String::from))
                .unwrap_or_else(|| "(unknown account)".to_string());
            ok("auth", format!("logged in as {email} ({source})"))
        }
        Ok(resp) if resp.status().as_u16() == 401 => fail(
            "auth",
            format!("token ({source}) is invalid or expired"),
            "Run `stepshots login` again, or refresh STEPSHOTS_TOKEN.",
        ),
        Ok(resp) => warn(
            "auth",
            format!("whoami returned HTTP {}", resp.status().as_u16()),
            "",
        ),
        Err(e) => warn(
            "auth",
            format!("could not validate token against {server}: {e}"),
            "",
        ),
    }
}
