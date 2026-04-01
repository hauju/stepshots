use manifest::Viewport;

use crate::actions::execute_action;
use crate::browser::Browser;
use crate::config::StepshotsConfig;
use crate::error::CliError;

/// Run a tutorial in a visible browser for preview/debugging.
pub async fn run(
    config: &StepshotsConfig,
    tutorial_key: &str,
    viewport: &Viewport,
) -> Result<(), CliError> {
    let tutorial = config.tutorials.get(tutorial_key).ok_or_else(|| {
        CliError::Config(format!(
            "Tutorial '{tutorial_key}' not found. Available: {}",
            config
                .tutorials
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;

    println!("Preview: {} ({})", tutorial.title, tutorial_key);
    println!("Browser will stay open — press Ctrl+C to close.");

    let browser = Browser::launch(viewport, false).await?;

    let start_url = resolve_url(&config.base_url, &tutorial.url);
    browser.navigate(&start_url).await?;
    browser.wait_idle(config.default_delay).await;

    for (i, step) in tutorial.steps.iter().enumerate() {
        println!(
            "  Step {}: {} {}",
            i + 1,
            step.action,
            step.selector.as_deref().unwrap_or("")
        );
        execute_action(&browser, step, &config.base_url).await?;
        let delay = step.delay.unwrap_or(config.default_delay);
        browser.wait_idle(delay).await;
    }

    println!("All steps executed. Browser is open — press Ctrl+C to exit.");

    // Wait until interrupted
    tokio::signal::ctrl_c()
        .await
        .map_err(|e| CliError::Other(format!("Signal error: {e}")))?;

    Ok(())
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
