use crate::error::CliError;

/// Print the JSON Schema for `stepshots.config.json` to stdout.
pub fn run() -> Result<(), CliError> {
    println!("{}", generate()?);
    Ok(())
}

/// Generate the pretty-printed JSON Schema for the config file format.
pub fn generate() -> Result<String, CliError> {
    let schema = schemars::schema_for!(manifest::StepshotsConfig);
    Ok(serde_json::to_string_pretty(&schema)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema checked into `schema/stepshots.config.schema.json` (the file
    /// `$schema` URLs point at) must match what the current types generate.
    /// Regenerate with: cargo run -- schema > schema/stepshots.config.schema.json
    #[test]
    fn checked_in_schema_is_current() {
        let generated = generate().unwrap();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../schema/stepshots.config.schema.json"
        );
        let on_disk = std::fs::read_to_string(path).expect(
            "schema/stepshots.config.schema.json missing — regenerate with `cargo run -- schema`",
        );
        assert_eq!(
            on_disk.trim(),
            generated.trim(),
            "schema file is stale — regenerate with `cargo run -- schema > schema/stepshots.config.schema.json`"
        );
    }
}
