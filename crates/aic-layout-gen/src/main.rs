use aic_layout_gen::{Config, batch, io};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Generate and inspect complete procedural physical layouts")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write an editable generator configuration; no game data is embedded.
    Config {
        #[arg(long)]
        output: PathBuf,
    },
    /// Produce fully routed layouts, failure records and an interactive gallery.
    Generate {
        #[arg(long)]
        config: PathBuf,
        #[arg(long, default_value = "data/game/normalized")]
        catalog_dir: PathBuf,
        #[arg(long)]
        seed: u64,
        #[arg(long, default_value_t = 1)]
        count: usize,
        #[arg(long, default_value_t = 1)]
        workers: usize,
        #[arg(long)]
        output: PathBuf,
    },
    /// Independently verify a persisted physical layout against external catalogs.
    Validate {
        #[arg(long)]
        layout: PathBuf,
        #[arg(long, default_value = "data/game/normalized")]
        catalog_dir: PathBuf,
    },
}

fn run() -> anyhow::Result<bool> {
    match Cli::parse().command {
        Command::Config { output } => {
            io::write_json(&output, &Config::default())?;
            println!("{}", output.display());
        }
        Command::Generate {
            config,
            catalog_dir,
            seed,
            count,
            workers,
            output,
        } => {
            let config: Config = io::read(&config)?;
            let report = batch::run(config, &catalog_dir, seed, count, workers, &output)?;
            println!(
                "{}",
                serde_json::json!({"complete":report.complete,"requested":report.requested,
                "elapsed_seconds":report.elapsed_seconds,"complete_per_second":report.complete_per_second,
                "report":output.join("report.json"),"gallery":output.join("index.html")})
            );
            return Ok(report.complete == report.requested);
        }
        Command::Validate {
            layout,
            catalog_dir,
        } => {
            let loaded = io::load_catalogs(&catalog_dir)?;
            let value = io::read(&layout)?;
            let metrics =
                aic_layout::validation::validate(&value, &loaded.catalogs).map_err(|d| {
                    anyhow::anyhow!(
                        "{}: {}",
                        layout.display(),
                        serde_json::to_string(&d).unwrap()
                    )
                })?;
            println!(
                "{}",
                serde_json::json!({"physical_validated":true,"layout":layout,"metrics":metrics})
            );
        }
    }
    Ok(true)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(2),
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::json!({"code":"aic-layout-gen.command-failed","severity":"error","stage":"cli","message":format!("{error:#}")})
            );
            std::process::exit(1);
        }
    }
}
