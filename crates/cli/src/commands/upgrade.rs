use std::time::Duration;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use semver::Version;

use crate::error::CliError;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "hauju/stepshots";
const CARGO_INSTALL_URL: &str = "https://github.com/hauju/stepshots.git";
const PACKAGE_NAME: &str = "stepshots-cli";

pub async fn run(force: bool, check_only: bool) -> Result<(), CliError> {
    let current = Version::parse(CURRENT_VERSION)
        .map_err(|e| CliError::Upgrade(format!("Failed to parse current version: {e}")))?;

    println!("  {} Checking for updates...", style("●").dim(),);

    let latest_str = match fetch_latest_version().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "  {} Could not check for updates: {e}",
                style("✗").red().bold(),
            );
            if force {
                eprintln!(
                    "  {} Proceeding with reinstall (--force)",
                    style("→").cyan(),
                );
                return do_install(CURRENT_VERSION, None).await;
            }
            return Err(e);
        }
    };

    let latest = Version::parse(&latest_str).map_err(|e| {
        CliError::Upgrade(format!(
            "Failed to parse latest version '{latest_str}': {e}"
        ))
    })?;

    if current >= latest && !force {
        println!(
            "  {} stepshots is already at the latest version (v{})",
            style("✓").green().bold(),
            current,
        );
        return Ok(());
    }

    if current < latest {
        println!(
            "  {} Update available: v{} → v{}",
            style("✓").green().bold(),
            current,
            latest,
        );
    }

    if check_only {
        return Ok(());
    }

    do_install(CURRENT_VERSION, Some(&latest_str)).await
}

async fn do_install(current: &str, latest: Option<&str>) -> Result<(), CliError> {
    verify_cargo_available()?;

    let target = latest.unwrap_or(current);
    println!(
        "  {} Upgrading stepshots... v{} → v{}",
        style("→").cyan(),
        current,
        target,
    );

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("valid template"),
    );
    spinner.set_message("Compiling from source (this may take a minute)...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    let output = tokio::process::Command::new("cargo")
        .args([
            "install",
            "--git",
            CARGO_INSTALL_URL,
            PACKAGE_NAME,
            "--force",
        ])
        .output()
        .await
        .map_err(|e| CliError::Upgrade(format!("Failed to run cargo install: {e}")))?;

    spinner.finish_and_clear();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Upgrade(format!(
            "cargo install failed:\n{stderr}"
        )));
    }

    println!(
        "  {} Successfully upgraded to stepshots v{}",
        style("✓").green().bold(),
        target,
    );

    Ok(())
}

async fn fetch_latest_version() -> Result<String, CliError> {
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .header("User-Agent", format!("stepshots-cli/{CURRENT_VERSION}"))
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
        .map_err(|e| CliError::Upgrade(format!("Could not reach GitHub: {e}")))?;

    if resp.status().as_u16() == 404 {
        return Err(CliError::Upgrade(
            "No releases found on GitHub. Upgrade manually with: cargo install --git https://github.com/hauju/stepshots.git stepshots-cli --force".into(),
        ));
    }

    if !resp.status().is_success() {
        return Err(CliError::Upgrade(format!(
            "GitHub API returned status {}",
            resp.status()
        )));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| CliError::Upgrade(format!("Failed to parse GitHub response: {e}")))?;

    let tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CliError::Upgrade("No tag_name in GitHub release".into()))?;

    Ok(tag.strip_prefix('v').unwrap_or(tag).to_string())
}

fn verify_cargo_available() -> Result<(), CliError> {
    match std::process::Command::new("cargo")
        .arg("--version")
        .output()
    {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(CliError::Upgrade(
            "cargo not found. Install Rust from https://rustup.rs".into(),
        )),
    }
}
