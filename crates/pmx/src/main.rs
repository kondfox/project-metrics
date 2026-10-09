use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use pm_config::Workspace;
use pm_metrics::ProjectFile;
use pmx::export::{Cadence, long_rows, to_csv};
use pmx::{CollectOptions, collect, write_outputs};

#[derive(Parser)]
#[command(name = "pmx", version, about = "Engineering metrics from git history")]
struct Cli {
    /// Workspace folder (holds pmx.toml, .pmx/ and out/).
    #[arg(short = 'C', long, global = true, default_value = ".")]
    workspace: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Read the repos and write out/project.json and out/private/leads.json.
    Collect {
        /// Measure as if today were this date (YYYY-MM-DD); default: today.
        #[arg(long)]
        as_of: Option<NaiveDate>,
        /// Don't `git fetch`; measure the refs that are already there.
        #[arg(long)]
        no_fetch: bool,
        /// Ignore and don't update .pmx/cache.sqlite.
        #[arg(long)]
        no_cache: bool,
    },
    /// Print out/project.json in another format.
    Export {
        #[arg(long, value_enum)]
        format: Format,
        #[arg(long, conflicts_with = "monthly")]
        weekly: bool,
        #[arg(long)]
        monthly: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// project,metric,period,value,n (parity.md §1.1)
    Long,
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Collect {
            as_of,
            no_fetch,
            no_cache,
        } => {
            let ws = Workspace::load(&cli.workspace)?;
            let mut opts = CollectOptions::new(as_of.unwrap_or_else(|| chrono::Local::now().date_naive()));
            opts.fetch = !no_fetch;
            opts.use_cache = !no_cache;
            opts.log = true;
            eprintln!("pmx collect · {} (as of {})", ws.config.project.name, opts.as_of);
            let c = collect(&ws, &opts)?;
            for w in &c.warnings {
                eprintln!("warning: {w}");
            }
            let out = ws.out_dir();
            write_outputs(&out, &c)?;
            eprintln!("wrote {}", out.join("project.json").display());
        }
        Command::Export {
            format,
            weekly,
            monthly,
        } => {
            let path = cli.workspace.join("out").join("project.json");
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {} (run `pmx collect` first)", path.display()))?;
            let p: ProjectFile = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
            if weekly && monthly {
                bail!("choose --weekly or --monthly");
            }
            let cadence = if weekly { Cadence::Weekly } else { Cadence::Monthly };
            match format {
                Format::Long => print!("{}", to_csv(&long_rows(&p, cadence))),
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
