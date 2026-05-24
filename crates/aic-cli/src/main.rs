use std::{path::PathBuf, process::ExitCode};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "aic",
    version,
    about = "Generate layouts for Endfield AIC production plans."
)]
struct Cli {
    /// Directory containing external facility and recipe data.
    #[arg(long, value_name = "DIR", default_value = "data")]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Check whether the external data directory can be read.
    CheckData,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::CheckData => check_data(cli.data_dir),
    }
}

fn check_data(data_dir: PathBuf) -> Result<()> {
    let metadata = std::fs::metadata(&data_dir).with_context(|| {
        format!(
            "failed to access external data directory '{}'",
            data_dir.display()
        )
    })?;

    ensure!(
        metadata.is_dir(),
        "external data path '{}' is not a directory",
        data_dir.display()
    );

    println!("external data directory: {}", data_dir.display());
    Ok(())
}
