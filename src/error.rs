use std::process::ExitCode;

/// Errors returned by the CLI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Auth(#[from] crate::auth::Error),

    /// The command exists but its behavior has not landed yet.
    #[error("{0} is not implemented yet")]
    NotImplemented(&'static str),

    /// Configuration is missing or invalid.
    #[error("configuration error: {0}")]
    Config(String),

    /// An input/output operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON serialization or deserialization failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    /// Any other failure with a human-readable message.
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Process exit code for this error.
    pub fn exit_code(&self) -> ExitCode {
        match self {
            Self::Auth(crate::auth::Error::Failed(_) | crate::auth::Error::BrowserRequired) => {
                ExitCode::from(3)
            }
            Self::Auth(crate::auth::Error::Network | crate::auth::Error::RateLimited) => {
                ExitCode::from(4)
            }
            Self::Auth(crate::auth::Error::Upstream(_)) => ExitCode::from(5),
            _ => ExitCode::FAILURE,
        }
    }
}
