use std::path::Path;

use crate::config::sample_config;
use crate::error::CliError;

const CONFIG_FILENAME: &str = "stepshots.config.json";

/// Generate a sample `stepshots.config.json` in the current directory.
pub fn run(force: bool) -> Result<(), CliError> {
    let path = Path::new(CONFIG_FILENAME);
    if path.exists() && !force {
        return Err(CliError::Config(format!(
            "{CONFIG_FILENAME} already exists. Use --force to overwrite."
        )));
    }

    let content = sample_config();
    std::fs::write(path, content)?;

    println!("Created {CONFIG_FILENAME}");
    println!("Edit it with your website URL and tutorial steps, then run:");
    println!("  stepshots record");

    Ok(())
}
