use std::io;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Config error: {0}")]
    Config(String),

    #[error("Browser error: {0}")]
    Browser(String),

    #[error("Action error: {0}")]
    Action(String),

    #[error("Bundle error: {0}")]
    Bundle(String),

    #[error("Upload error: {0}")]
    Upload(String),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Upgrade error: {0}")]
    Upgrade(String),

    #[error("{0}")]
    Other(String),
}

impl From<chromiumoxide::error::CdpError> for CliError {
    fn from(e: chromiumoxide::error::CdpError) -> Self {
        CliError::Browser(e.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(e: serde_json::Error) -> Self {
        CliError::Config(e.to_string())
    }
}

impl From<zip::result::ZipError> for CliError {
    fn from(e: zip::result::ZipError) -> Self {
        CliError::Bundle(e.to_string())
    }
}

impl From<reqwest::Error> for CliError {
    fn from(e: reqwest::Error) -> Self {
        CliError::Upload(e.to_string())
    }
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Config(_) | CliError::Io(_) | CliError::Upgrade(_) | CliError::Other(_) => 1,
            CliError::Browser(_) => 2,
            CliError::Action(_) => 3,
            CliError::Bundle(_) => 4,
            CliError::Upload(_) | CliError::Auth(_) => 5,
        }
    }

    pub fn error_category(&self) -> &'static str {
        match self {
            CliError::Config(_) => "config",
            CliError::Browser(_) => "browser",
            CliError::Action(_) => "action",
            CliError::Bundle(_) => "bundle",
            CliError::Upload(_) => "upload",
            CliError::Auth(_) => "auth",
            CliError::Io(_) => "io",
            CliError::Upgrade(_) => "upgrade",
            CliError::Other(_) => "other",
        }
    }
}
