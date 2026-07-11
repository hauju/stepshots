use std::path::Path;

use manifest::Viewport;

use crate::browser::Browser;
use crate::error::CliError;

/// Open a visible browser on a persistent profile so the user can log in to
/// sites that authenticated recordings need. The session (cookies, local
/// storage) is saved in the profile directory and reused by `record`,
/// `preview`, and `inspect` when they are given the same `--profile-dir`.
pub async fn run(url: &str, profile_dir: &Path) -> Result<(), CliError> {
    let viewport = Viewport {
        width: 1280,
        height: 800,
        device_scale_factor: None,
    };

    println!("Opening browser with profile: {}", profile_dir.display());
    println!("Log in to the sites you want to record, then press Ctrl+C here when done.");

    let browser = Browser::launch(&viewport, false, Some(profile_dir)).await?;
    browser.navigate(url).await?;

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| CliError::Other(format!("Signal error: {e}")))?;

    println!("Profile saved: {}", profile_dir.display());
    Ok(())
}
