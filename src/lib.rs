//! SIMASTER feature modules and command-line adapter.

pub mod auth;
pub mod cli;
pub mod config;
pub mod error;
pub mod output;

use tracing_subscriber::EnvFilter;

use crate::cli::Cli;
use crate::error::Error;

/// Initialize logging and dispatch the selected command.
///
/// Programmatic callers can use the feature modules directly.
pub fn run_with(cli: Cli) -> Result<(), Error> {
    init_tracing(cli.verbose);
    cli::run(cli)
}

fn init_tracing(verbose: bool) {
    let default_filter = if verbose { "debug" } else { "warn" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_target(false)
        .init();
}
