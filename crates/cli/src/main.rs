mod actions;
mod auth;
mod browser;
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
    /// Log in to Stepshots via your browser (stores an API token locally)
    Login {
        /// Server URL
        #[arg(
            long,
            env = "STEPSHOTS_SERVER",
            default_value = "https://stepshots.com"
        )]
        server: String,
    },
    /// Remove locally stored Stepshots credentials
    Logout,
    /// Show which account you're logged in as
    Whoami {
        /// Server URL
        #[arg(
            long,
            env = "STEPSHOTS_SERVER",
            default_value = "https://stepshots.com"
        )]
        server: String,

        /// API token (defaults to the stored login token)
        #[arg(long, env = "STEPSHOTS_TOKEN")]
        token: Option<String>,
    },
    /// Generate a sample stepshots.config.json
    Init {
        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },
    /// Print the JSON Schema for stepshots.config.json
    Schema,
    /// Create, validate, and check guided-tour source files (*.tour.json)
    Tour {
        #[command(subcommand)]
        command: TourCommands,
    },
    /// Record tutorials into .stepshot bundles
    Record {
        /// Tutorials to record (by key). Records all if omitted.
        #[arg(value_name = "TUTORIAL")]
        tutorials: Vec<String>,

        /// Tutorial to record (same as the positional argument)
        #[arg(long, short, value_name = "TUTORIAL")]
        tutorial: Vec<String>,

        /// Output directory for .stepshot files
        #[arg(long, short, default_value = "output")]
        output: PathBuf,

        /// Show what would be recorded without launching a browser
        #[arg(long)]
        dry_run: bool,

        /// Persistent browser profile directory (for authenticated recordings)
        #[arg(long, env = "STEPSHOTS_PROFILE_DIR")]
        profile_dir: Option<PathBuf>,
    },
    /// List the tutorials defined in the config
    List,
    /// Preview a tutorial in a visible browser
    Preview {
        /// Tutorial key to preview
        tutorial: String,

        /// Persistent browser profile directory (for authenticated recordings)
        #[arg(long, env = "STEPSHOTS_PROFILE_DIR")]
        profile_dir: Option<PathBuf>,
    },
    /// Verify demos still match the live app (replays steps, writes no bundle)
    Verify {
        /// Tutorials to verify (by key). Verifies all if omitted.
        #[arg(value_name = "TUTORIAL")]
        tutorials: Vec<String>,

        /// Tutorial to verify (same as the positional argument)
        #[arg(long, short, value_name = "TUTORIAL")]
        tutorial: Vec<String>,

        /// Directory for failure screenshots
        #[arg(long, default_value = "output")]
        save_failures: PathBuf,

        /// Exit non-zero on: fail (broken steps) or warn (also annotation drift)
        #[arg(long, value_enum, default_value = "fail")]
        fail_on: commands::verify::FailOn,

        /// Persistent browser profile directory (for authenticated flows)
        #[arg(long, env = "STEPSHOTS_PROFILE_DIR")]
        profile_dir: Option<PathBuf>,
    },
    /// Open a visible browser with a saved profile to log in to sites
    /// used by authenticated recordings
    Browser {
        /// URL to open
        #[arg(default_value = "about:blank")]
        url: String,

        /// Persistent browser profile directory to create or reuse
        #[arg(long, env = "STEPSHOTS_PROFILE_DIR")]
        profile_dir: PathBuf,
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

        /// Make new demos publicly viewable immediately (ignored with --demo-id)
        #[arg(long)]
        public: bool,

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

        /// Persistent browser profile directory (for authenticated pages)
        #[arg(long, env = "STEPSHOTS_PROFILE_DIR")]
        profile_dir: Option<PathBuf>,
    },
    /// Check your setup: browser, config, server, and login
    Doctor {
        /// Server URL
        #[arg(
            long,
            env = "STEPSHOTS_SERVER",
            default_value = "https://stepshots.com"
        )]
        server: String,
    },
    /// Generate shell completions (bash, zsh, fish, powershell, elvish)
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
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

#[derive(Subcommand)]
enum TourCommands {
    /// Scaffold a guided-tour source file (optionally from a recorded bundle)
    Init {
        /// Tour key: the ?tour= URL parameter and registry key
        key: String,

        /// Project a recorded .stepshot bundle into the scaffold
        #[arg(long)]
        from: Option<PathBuf>,

        /// Output file (default: tours/<key>.tour.json)
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Overwrite an existing tour file
        #[arg(long)]
        force: bool,
    },
    /// Validate tour source files (strict parse + lints)
    Validate {
        /// Tour files or directories (default: tours/)
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
    },
    /// Replay tour files against a live app and report selector drift
    Check {
        /// Tour files or directories (default: tours/)
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Base URL of the app to replay against (e.g. a staging deploy)
        #[arg(long)]
        url: String,

        /// Exit non-zero on: fail (broken steps) or warn (also fallback drift)
        #[arg(long, value_enum, default_value = "fail")]
        fail_on: commands::verify::FailOn,

        /// Rewrite each selector-resolved step's fallback anchors from the live DOM
        #[arg(long)]
        update_fallbacks: bool,

        /// Persistent browser profile directory (for authenticated apps)
        #[arg(long, env = "STEPSHOTS_PROFILE_DIR")]
        profile_dir: Option<PathBuf>,
    },
    /// Merge tour files into a window.__STEPSHOTS_TOURS registry script
    Build {
        /// Tour files or directories (default: tours/)
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Output script path
        #[arg(long, short, default_value = "tours.js")]
        output: PathBuf,
    },
    /// Push tour files to Stepshots hosting (upsert by key; git stays canonical)
    Push {
        /// Tour files or directories (default: tours/)
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Server URL
        #[arg(
            long,
            env = "STEPSHOTS_SERVER",
            default_value = "https://stepshots.com"
        )]
        server: String,

        /// API token (defaults to the stored login token)
        #[arg(long, env = "STEPSHOTS_TOKEN")]
        token: Option<String>,
    },
    /// Print the JSON Schema for *.tour.json files
    Schema,
    /// Deprecated: use `tour init --from <bundle>` or `tour build`
    Export {
        /// Path to the .stepshot bundle
        bundle: PathBuf,

        /// Output file (default: <bundle>.tour.json)
        #[arg(long, short)]
        output: Option<PathBuf>,

        /// Key to register the tour under (default: bundle filename stem)
        #[arg(long)]
        key: Option<String>,

        /// Output format: json (registry) or js (assigns window.__STEPSHOTS_TOURS)
        #[arg(long, value_enum, default_value = "json")]
        format: commands::tour::TourFormat,
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
        if let CliError::Reported { code } = e {
            std::process::exit(code);
        }
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
        Commands::Login { server } => {
            auth::login(&server).await?;
        }
        Commands::Logout => {
            auth::logout()?;
        }
        Commands::Whoami { server, token } => {
            auth::whoami(&server, token).await?;
        }
        Commands::Init { force } => {
            commands::init::run(force)?;
        }
        Commands::Schema => {
            commands::schema::run()?;
        }
        Commands::Tour { command } => match command {
            TourCommands::Init {
                key,
                from,
                output,
                force,
            } => {
                commands::tour::init(&key, from.as_deref(), output, force, json)?;
            }
            TourCommands::Validate { paths } => {
                commands::tour::validate(&paths, json)?;
            }
            TourCommands::Check {
                paths,
                url,
                fail_on,
                update_fallbacks,
                profile_dir,
            } => {
                commands::tour_check::run(
                    &paths,
                    &url,
                    fail_on,
                    update_fallbacks,
                    profile_dir.as_deref(),
                    json,
                )
                .await?;
            }
            TourCommands::Build { paths, output } => {
                commands::tour::build(&paths, &output, json)?;
            }
            TourCommands::Push {
                paths,
                server,
                token,
            } => {
                let token = token
                    .or_else(|| auth::stored_token_for(&server))
                    .ok_or_else(|| {
                        CliError::Auth(
                            "No API token. Run `stepshots login`, or set STEPSHOTS_TOKEN / use --token."
                                .into(),
                        )
                    })?;
                commands::tour::push(&paths, &server, &token, json).await?;
            }
            TourCommands::Schema => {
                println!("{}", commands::schema::generate_tour()?);
            }
            TourCommands::Export {
                bundle,
                output,
                key,
                format,
            } => {
                commands::tour::export(&bundle, output, key, format, json)?;
            }
        },
        Commands::Record {
            tutorials,
            tutorial,
            output,
            dry_run,
            profile_dir,
        } => {
            let mut selected = tutorials;
            selected.extend(tutorial);
            let config_path = config::find_config(cli.config.as_deref())?;
            let config = config::load_config(&config_path)?;
            if !json {
                println!("Using config: {}", config_path.display());
            }
            commands::record::run(
                &config,
                &selected,
                &output,
                dry_run,
                json,
                profile_dir.as_deref(),
            )
            .await?;
        }
        Commands::List => {
            let config_path = config::find_config(cli.config.as_deref())?;
            let config = config::load_config(&config_path)?;
            commands::list::run(&config, &config_path, json)?;
        }
        Commands::Preview {
            tutorial,
            profile_dir,
        } => {
            let config_path = config::find_config(cli.config.as_deref())?;
            let config = config::load_config(&config_path)?;
            if !json {
                println!("Using config: {}", config_path.display());
            }
            let effective_viewport =
                manifest::resolve_viewport(config.format.as_ref(), &config.viewport);
            commands::preview::run(
                &config,
                &tutorial,
                &effective_viewport,
                profile_dir.as_deref(),
            )
            .await?;
        }
        Commands::Verify {
            tutorials,
            tutorial,
            save_failures,
            fail_on,
            profile_dir,
        } => {
            let mut selected = tutorials;
            selected.extend(tutorial);
            let config_path = config::find_config(cli.config.as_deref())?;
            let config = config::load_config(&config_path)?;
            if !json {
                println!("Using config: {}", config_path.display());
            }
            commands::verify::run(
                &config,
                &config_path,
                &selected,
                &save_failures,
                fail_on,
                json,
                profile_dir.as_deref(),
            )
            .await?;
        }
        Commands::Browser { url, profile_dir } => {
            commands::browser::run(&url, &profile_dir).await?;
        }
        Commands::Upload {
            files,
            title,
            demo_id,
            public,
            server,
            token,
        } => {
            let token = token
                .or_else(|| auth::stored_token_for(&server))
                .ok_or_else(|| {
                    CliError::Auth(
                        "No API token. Run `stepshots login`, or set STEPSHOTS_TOKEN / use --token."
                            .into(),
                    )
                })?;
            commands::upload::run(
                &files,
                title.as_deref(),
                demo_id.as_deref(),
                public,
                json,
                &server,
                &token,
            )
            .await?;
        }
        Commands::Inspect {
            url,
            width,
            height,
            profile_dir,
        } => {
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
            commands::inspect::run(&url, width, height, json, profile_dir.as_deref()).await?;
        }
        Commands::Doctor { server } => {
            commands::doctor::run(&server, cli.config.as_deref(), json).await?;
        }
        Commands::Completions { shell } => {
            use clap::CommandFactory;
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "stepshots",
                &mut std::io::stdout(),
            );
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
