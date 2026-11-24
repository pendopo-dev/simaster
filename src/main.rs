mod cli;
mod config;
mod error;
mod output;

use std::process::ExitCode;

use clap::Parser;
use simaster_cli::auth;
use tracing_subscriber::{EnvFilter, filter::filter_fn, prelude::*};

use crate::cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);
    match cli::run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            error.exit_code()
        }
    }
}

fn init_tracing(verbose: bool) {
    let default_filter = if verbose { "debug" } else { "warn" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    let console = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_filter(filter_fn(|metadata| {
            matches!(
                metadata.target().split("::").next(),
                Some("simaster" | "simaster_cli")
            )
        }));
    tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .init();
}
