mod actions;
mod browser;
mod bundle_reader;
mod bundler;
mod commands;
mod config;
mod error;
pub mod output;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::error::CliError;

#[derive(Parser)]
#[command(
    name = "stepshots",
    about = "Record, bundle, and upload interactive product demos",
    version
)]
struct Cli {
    /// Path to config file (default: auto-detect stepshots.config.json)
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Output results as JSON to stdout (for AI agents and automation)
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a sample stepshots.config.json
    Init {
        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },
    /// Record tutorials into .stepshot bundles
    Record {
        /// Record only specific tutorials (by key). Records all if omitted.
        #[arg(long, short)]
        tutorial: Vec<String>,

        /// Output directory for .stepshot files
        #[arg(long, short, default_value = "output")]
        output: PathBuf,

        /// Show what would be recorded without launching a browser
        #[arg(long)]
        dry_run: bool,
    },
    /// Preview a tutorial in a visible browser
    Preview {
        /// Tutorial key to preview
        tutorial: String,
    },
    /// Re-record a .stepshot bundle with fresh screenshots
    ReRecord {
        /// Path to existing .stepshot bundle
        bundle: PathBuf,

        /// Override base URL (e.g. for staging/CI environments)
        #[arg(long)]
        base_url: Option<String>,

        /// Output directory for the new bundle
        #[arg(long, short, default_value = "output")]
        output: PathBuf,

        /// Show browser window (non-headless)
        #[arg(long)]
        headed: bool,

        /// Default delay between steps in ms
        #[arg(long, default_value = "500")]
        delay: u64,
    },
    /// Upload .stepshot bundles to the Stepshots API
    Upload {
        /// .stepshot files to upload
        files: Vec<String>,

        /// Override the demo title
        #[arg(long)]
        title: Option<String>,

        /// Replace an existing demo instead of creating a new one
        #[arg(long)]
        demo_id: Option<String>,

        /// Server URL
        #[arg(
            long,
            env = "STEPSHOTS_SERVER",
            default_value = "https://stepshots.com"
        )]
        server: String,

        /// API token
        #[arg(long, env = "STEPSHOTS_TOKEN")]
        token: Option<String>,
    },
    /// Inspect a page to discover interactive elements and CSS selectors
    Inspect {
        /// URL to inspect (defaults to config baseUrl if omitted)
        url: Option<String>,
        /// Viewport width
        #[arg(long, default_value = "1280")]
        width: u32,
        /// Viewport height
        #[arg(long, default_value = "800")]
        height: u32,
    },
    /// Upgrade stepshots to the latest version
    Upgrade {
        /// Force reinstall even if already on the latest version
        #[arg(long)]
        force: bool,

        /// Only check for updates without installing
        #[arg(long)]
        check: bool,
    },
    /// Start a local HTTP server for browser extension integration
    Serve {
        /// Port to listen on
        #[arg(long, short, default_value = "8124")]
        port: u16,

        /// Output directory for recorded bundles
        #[arg(long, short, default_value = "output")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    if cli.verbose {
        tracing_subscriber::fmt()
            .with_env_filter("stepshots=debug")
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("stepshots=info")
            .init();
    }

    let json = cli.json;
    if let Err(e) = run(cli).await {
        if json {
            let output = serde_json::json!({
                "success": false,
                "error": {
                    "category": e.error_category(),
                    "message": e.to_string()
                }
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
        } else {
            eprintln!("Error: {e}");
        }
        std::process::exit(e.exit_code());
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let json = cli.json;
    match cli.command {
        Commands::Init { force } => {
            commands::init::run(force)?;
        }
        Commands::Record {
            tutorial,
            output,
            dry_run,
        } => {
            let config_path = config::find_config(cli.config.as_deref())?;
            let config = config::load_config(&config_path)?;
            if !json {
                println!("Using config: {}", config_path.display());
            }
            commands::record::run(&config, &tutorial, &output, dry_run, json).await?;
        }
        Commands::Preview { tutorial } => {
            let config_path = config::find_config(cli.config.as_deref())?;
            let config = config::load_config(&config_path)?;
            if !json {
                println!("Using config: {}", config_path.display());
            }
            let effective_viewport =
                manifest::resolve_viewport(config.format.as_ref(), &config.viewport);
            commands::preview::run(&config, &tutorial, &effective_viewport).await?;
        }
        Commands::ReRecord {
            bundle,
            base_url,
            output,
            headed,
            delay,
        } => {
            commands::rerecord::run(&bundle, base_url.as_deref(), &output, headed, delay, json)
                .await?;
        }
        Commands::Upload {
            files,
            title,
            demo_id,
            server,
            token,
        } => {
            let token = token.ok_or_else(|| {
                CliError::Auth("No API token provided. Set STEPSHOTS_TOKEN or use --token.".into())
            })?;
            commands::upload::run(
                &files,
                title.as_deref(),
                demo_id.as_deref(),
                &server,
                &token,
            )
            .await?;
        }
        Commands::Inspect { url, width, height } => {
            let url = match url {
                Some(u) => u,
                None => {
                    let config_path = config::find_config(cli.config.as_deref())?;
                    let config = config::load_config(&config_path)?;
                    if !json {
                        println!("Using config: {}", config_path.display());
                    }
                    config.base_url.clone()
                }
            };
            commands::inspect::run(&url, width, height, json).await?;
        }
        Commands::Upgrade { force, check } => {
            commands::upgrade::run(force, check).await?;
        }
        Commands::Serve { port, output } => {
            commands::serve::run(port, output).await?;
        }
    }

    Ok(())
}
