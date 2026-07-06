use std::path::PathBuf;
use std::time::Duration;

use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use semver::Version;

use crate::error::CliError;

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "hauju/stepshots";
const CARGO_INSTALL_URL: &str = "https://github.com/hauju/stepshots.git";
const PACKAGE_NAME: &str = "stepshots-cli";
const INSTALL_SH_URL: &str = "https://raw.githubusercontent.com/hauju/stepshots/main/install.sh";

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
    let target = latest.unwrap_or(current);
    println!(
        "  {} Upgrading stepshots... v{} → v{}",
        style("→").cyan(),
        current,
        target,
    );

    // Cargo installs are upgraded from source; binary installs (via install.sh)
    // pull the matching prebuilt release. Windows only ships via cargo.
    if installed_via_cargo() || cfg!(not(unix)) {
        upgrade_via_cargo().await?;
    } else {
        upgrade_via_installer(latest).await?;
    }

    println!(
        "  {} Successfully upgraded to stepshots v{}",
        style("✓").green().bold(),
        target,
    );

    Ok(())
}

/// Upgrade a source install with `cargo install --git`.
async fn upgrade_via_cargo() -> Result<(), CliError> {
    verify_cargo_available()?;

    let spinner = new_spinner("Compiling from source (this may take a minute)...");
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

    Ok(())
}

/// Upgrade a binary install by re-running install.sh, which downloads the
/// matching prebuilt release into the current binary's directory.
async fn upgrade_via_installer(version: Option<&str>) -> Result<(), CliError> {
    let exe = std::env::current_exe().map_err(|e| {
        CliError::Upgrade(format!("could not locate the stepshots executable: {e}"))
    })?;
    let install_dir = exe
        .parent()
        .ok_or_else(|| CliError::Upgrade("could not determine the install directory".into()))?;

    let spinner = new_spinner("Downloading the latest release...");

    // Force install.sh's temp dir onto the same filesystem as the destination so
    // its final `mv` is an atomic rename() — overwriting the running binary via a
    // cross-filesystem copy would otherwise fail with ETXTBSY on Linux. mktemp
    // honours TMPDIR. `set -e` + assignment ensures a curl failure aborts before
    // the download is piped into sh.
    let mut cmd = tokio::process::Command::new("sh");
    cmd.arg("-c")
        .arg(format!(
            "set -e; script=\"$(curl -fsSL {INSTALL_SH_URL})\"; printf %s \"$script\" | sh"
        ))
        .env("STEPSHOTS_INSTALL_DIR", install_dir)
        .env("TMPDIR", install_dir);
    if let Some(v) = version {
        cmd.env("STEPSHOTS_VERSION", format!("v{v}"));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| CliError::Upgrade(format!("failed to run the installer: {e}")))?;
    spinner.finish_and_clear();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Upgrade(format!(
            "installer failed:\n{}",
            stderr.trim()
        )));
    }

    Ok(())
}

/// Whether the running binary lives in Cargo's bin directory.
fn installed_via_cargo() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let cargo_bin = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".cargo")))
        .map(|c| c.join("bin"));
    matches!(cargo_bin, Some(dir) if exe.starts_with(&dir))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn new_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .expect("valid template"),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));
    spinner
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
