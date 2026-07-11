use std::path::Path;

use crate::config::StepshotsConfig;
use crate::error::CliError;
use crate::output::{ListOutput, ListTutorial};

/// List the tutorials defined in the config.
pub fn run(config: &StepshotsConfig, config_path: &Path, json: bool) -> Result<(), CliError> {
    let mut keys: Vec<&String> = config.tutorials.keys().collect();
    keys.sort();

    if json {
        let tutorials: Vec<ListTutorial> = keys
            .iter()
            .map(|key| {
                let t = &config.tutorials[*key];
                ListTutorial {
                    key: (*key).clone(),
                    title: t.title.clone(),
                    description: t.description.clone(),
                    steps: t.steps.len(),
                }
            })
            .collect();
        let out = ListOutput {
            success: true,
            command: "list",
            config: config_path.display().to_string(),
            tutorials,
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&out).expect("serializing ListOutput")
        );
        return Ok(());
    }

    println!("Tutorials in {}:", config_path.display());
    let width = keys.iter().map(|k| k.len()).max().unwrap_or(0);
    for key in &keys {
        let t = &config.tutorials[*key];
        println!(
            "  {key:<width$}  {} ({} steps)",
            t.title,
            t.steps.len(),
            key = key,
            width = width
        );
    }
    println!();
    println!("Record one with: stepshots record <key>");

    Ok(())
}
