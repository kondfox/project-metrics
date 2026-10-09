use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};
use pm_config::edit::ConfigDoc;
use pm_config::import::{SideFiles, from_prototype};
use pm_config::{CONFIG_FILE, Config, RepoRole, Workspace};
use pm_git::Git;
use pm_metrics::ProjectFile;
use pm_progress::{Mode, Progress, fmt_n};
use pmx::collect::{plan, plan_progress};
use pmx::export::{Cadence, long_rows, to_csv};
use pmx::people::{Status, add_unique, gather, merge, propose};
use pmx::setup::{
    CONFIG_HEADER, RepoLocation, dedupe_names, default_range_start, looks_like_url, new_config, scan, suggest,
};
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
    /// Create pmx.toml from the git clones found in DIRS (default: the current folder).
    Init {
        dirs: Vec<PathBuf>,
        /// Project name (default: the workspace folder's name).
        #[arg(long)]
        name: Option<String>,
        /// First day measured, YYYY-MM-DD (default: the first of the month a year ago).
        #[arg(long)]
        since: Option<NaiveDate>,
        /// Write without asking.
        #[arg(long, short)]
        yes: bool,
        /// Overwrite an existing pmx.toml.
        #[arg(long)]
        force: bool,
    },
    /// Add, list, change or remove repos.
    #[command(subcommand)]
    Repo(RepoCommand),
    /// Show every author identity and propose [people] merges.
    People {
        #[command(subcommand)]
        action: Option<PeopleCommand>,
        /// Write the proposed merges into pmx.toml.
        #[arg(long)]
        apply: bool,
    },
    /// Check paths, revisions, tokens, the data policy and unmapped identities.
    Check {
        /// Unmapped identities with at least this many commits are errors.
        #[arg(long, default_value_t = 10)]
        threshold: u64,
    },
    /// Convert a prototype config_*.json (plus externals.json, fte.json, secrets_triage.json).
    ImportConfig {
        config: PathBuf,
        /// Default: externals.json next to the config or in ../tools.
        #[arg(long)]
        externals: Option<PathBuf>,
        #[arg(long)]
        fte: Option<PathBuf>,
        #[arg(long)]
        secrets_triage: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    /// Read the repos and write out/project.json and out/private/leads.json.
    Collect {
        #[command(flatten)]
        run: RunArgs,
        /// Print the work and the ETA without running anything.
        #[arg(long)]
        dry_run: bool,
        /// Ignore and don't update .pmx/cache.sqlite.
        #[arg(long)]
        no_cache: bool,
        /// Recompute every repo instead of continuing from the cached SHA.
        #[arg(long)]
        full: bool,
        /// auto, tty, plain (a line every 10 s) or json (events on stdout).
        #[arg(long, default_value = "auto")]
        progress: String,
    },
    /// Same as `collect --dry-run`.
    Plan {
        #[command(flatten)]
        run: RunArgs,
    },
    /// Serve the dashboard on http://127.0.0.1 (re-reads out/ on every reload; includes the
    /// private lead view, since only this machine can connect).
    Serve {
        #[arg(long, default_value_t = 7878)]
        port: u16,
        /// Open it in the default browser.
        #[arg(long)]
        open: bool,
    },
    /// Write out/project.json in another format: `long` (CSV to stdout) or `html` (the dashboard
    /// as one self-contained file).
    Export {
        #[arg(long, value_enum)]
        format: Format,
        #[arg(long, conflicts_with = "monthly")]
        weekly: bool,
        #[arg(long)]
        monthly: bool,
        /// html: where to write (default out/dashboard.html).
        #[arg(long, short)]
        output: Option<PathBuf>,
        /// html: include the per-person lead view (out/private/leads.json). Don't share that file.
        #[arg(long)]
        with_private: bool,
    },
}

#[derive(Args)]
struct RunArgs {
    /// Measure as if today were this date (YYYY-MM-DD); default: today.
    #[arg(long)]
    as_of: Option<NaiveDate>,
    /// Don't `git fetch`; measure the refs that are already there.
    #[arg(long)]
    no_fetch: bool,
}

#[derive(Subcommand)]
enum RepoCommand {
    /// Add a local clone or a URL (pmx keeps its own read-only clone).
    Add {
        location: String,
        #[arg(long)]
        role: Option<RepoRole>,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    List,
    /// Change a repo's role, branch, pinned rev or name.
    Set {
        name: String,
        #[arg(long)]
        role: Option<RepoRole>,
        #[arg(long)]
        branch: Option<String>,
        /// Pin a commit; `--rev none` unpins.
        #[arg(long)]
        rev: Option<String>,
        #[arg(long)]
        rename: Option<String>,
    },
    Remove {
        name: String,
    },
}

#[derive(Subcommand)]
enum PeopleCommand {
    /// Put these emails/logins under PERSON (created if new).
    Merge { person: String, ids: Vec<String> },
    /// Exclude these emails or names as bots.
    Bot { ids: Vec<String> },
    /// Exclude these names or emails as external developers.
    External { ids: Vec<String> },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// project,metric,period,value,n (parity.md §1.1)
    Long,
    /// The dashboard as one HTML file.
    Html,
}

fn confirm(question: &str, default: bool) -> bool {
    if !std::io::stdin().is_terminal() {
        return default;
    }
    eprint!("{question} [{}] ", if default { "Y/n" } else { "y/N" });
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return default;
    }
    match line.trim().to_lowercase().as_str() {
        "" => default,
        a => a.starts_with('y'),
    }
}

fn config_file(ws: &Path) -> PathBuf {
    ws.join(CONFIG_FILE)
}

/// Write a new pmx.toml (and a .gitignore for the workspace's private folders).
fn write_new_config(ws: &Path, config: &Config, force: bool) -> Result<()> {
    let path = config_file(ws);
    if path.exists() && !force {
        bail!("{} exists (use --force to overwrite)", path.display());
    }
    std::fs::create_dir_all(ws)?;
    std::fs::write(&path, format!("{CONFIG_HEADER}{}", config.to_toml()))?;
    let gi = ws.join(".gitignore");
    if !gi.exists() {
        std::fs::write(
            &gi,
            "# pmx cache, clones and outputs (outputs hold per-person data)\n.pmx/\nout/\n",
        )?;
    }
    eprintln!("wrote {}", path.display());
    Ok(())
}

fn edit_config(ws: &Path, f: impl FnOnce(&mut ConfigDoc) -> Result<()>) -> Result<()> {
    let path = config_file(ws);
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc = ConfigDoc::parse(&text)?;
    f(&mut doc)?;
    std::fs::write(&path, doc.to_text())?;
    eprintln!("updated {}", path.display());
    Ok(())
}

/// A pinned 40-hex SHA is shortened for display.
fn short_rev(rev: &str) -> String {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        rev[..12].to_string()
    } else {
        rev.to_string()
    }
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn print_repo_table(rows: &[(String, String, String, String)]) {
    let w0 = rows.iter().map(|r| r.0.chars().count()).max().unwrap_or(4).max(4);
    let w1 = rows.iter().map(|r| r.1.len()).max().unwrap_or(4).max(4);
    let w2 = rows.iter().map(|r| r.2.chars().count()).max().unwrap_or(6).max(6);
    for (a, b, c, d) in rows {
        println!("  {a:<w0$}  {b:<w1$}  {c:<w2$}  {d}");
    }
}

fn cmd_init(
    ws: &Path,
    dirs: Vec<PathBuf>,
    name: Option<String>,
    since: Option<NaiveDate>,
    yes: bool,
    force: bool,
) -> Result<()> {
    if config_file(ws).exists() && !force {
        bail!(
            "{} exists; edit it with `pmx repo` / `pmx people`, or use --force",
            config_file(ws).display()
        );
    }
    let dirs = if dirs.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        dirs
    };
    let found = scan(&dirs, &ws.join(".pmx"));
    let cls = pm_classify::Classifier::default();
    let mut suggestions = Vec::new();
    for dir in &found {
        match suggest(&Git::open(dir), RepoLocation::Path(dir.clone()), &cls) {
            Ok(s) => suggestions.push(s),
            Err(e) => eprintln!("skipping {}: {e:#}", dir.display()),
        }
    }
    dedupe_names(&mut suggestions);
    let ws_abs = std::fs::canonicalize(ws).unwrap_or_else(|_| ws.to_path_buf());
    let name = name.unwrap_or_else(|| {
        ws_abs
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".into())
    });
    let config = new_config(
        name,
        since.unwrap_or_else(|| default_range_start(today())),
        &suggestions,
    )?;

    println!("{} repos found:", suggestions.len());
    let rows: Vec<_> = suggestions
        .iter()
        .map(|s| {
            (
                s.repo.display_name(),
                s.repo.role.to_string(),
                s.repo.branch.clone().unwrap_or_default(),
                format!("{}  ({})", s.repo.path.clone().unwrap_or_default(), s.why),
            )
        })
        .collect();
    print_repo_table(&rows);
    println!(
        "project: {} · since {} · breadth roles: {}",
        config.project.name,
        config.project.range_start,
        config
            .project
            .breadth_roles
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    match &config.code_host {
        Some(h) => println!("code host: {:?} {}", h.kind, h.base.clone().unwrap_or_default()),
        None => println!("code host: not detected (set [code_host] by hand for PR metrics)"),
    }
    if !yes && !confirm("Write pmx.toml?", true) {
        bail!("nothing written");
    }
    write_new_config(ws, &config, force)?;
    println!("next: fix any role with `pmx repo set <name> --role …`, then `pmx people`, `pmx check`, `pmx collect`");
    Ok(())
}

fn cmd_repo(ws_dir: &Path, cmd: RepoCommand) -> Result<()> {
    let ws = Workspace::load(ws_dir)?;
    match cmd {
        RepoCommand::Add {
            location,
            role,
            branch,
            name,
        } => {
            let cls = ws.config.classifier(pm_classify::TestRules::Spec);
            let mut s = if looks_like_url(&location) {
                let mut probe = pm_config::RepoConfig {
                    url: Some(location.clone()),
                    name: name.clone(),
                    ..Default::default()
                };
                probe.name = Some(probe.display_name());
                let dir = ws.repo_dir(&probe);
                let git = if dir.exists() {
                    Git::open(&dir)
                } else {
                    Git::clone_bare(&location, &dir)?
                };
                suggest(&git, RepoLocation::Url(location.clone()), &cls)?
            } else {
                let dir = pm_config::expand_path(&location, &std::env::current_dir()?);
                suggest(&Git::open(&dir), RepoLocation::Path(dir), &cls)?
            };
            if let Some(r) = role {
                s.repo.role = r;
            }
            if let Some(b) = branch {
                s.repo.branch = Some(b);
            }
            s.repo.name = name;
            println!(
                "{}: role {} ({}), branch {}",
                s.repo.display_name(),
                s.repo.role,
                if role.is_some() {
                    "given".to_string()
                } else {
                    s.why.clone()
                },
                s.repo.branch.clone().unwrap_or_default()
            );
            edit_config(ws_dir, |d| Ok(d.add_repo(&s.repo)?))?;
        }
        RepoCommand::List => {
            let mut rows = Vec::new();
            for r in &ws.config.repos {
                let dir = ws.repo_dir(r);
                let rev = r.revision().map(String::from);
                let status = if !dir.exists() {
                    if r.url.is_some() {
                        "not cloned yet".to_string()
                    } else {
                        "MISSING".to_string()
                    }
                } else {
                    let git = Git::open(&dir);
                    let rev = rev.clone().unwrap_or_else(|| git.default_revision());
                    match git.rev_parse(&rev) {
                        Ok(sha) => format!("@ {}", &sha[..12]),
                        Err(_) => format!("`{rev}` does not resolve"),
                    }
                };
                let loc = r.path.clone().or(r.url.clone()).unwrap_or_default();
                rows.push((
                    r.display_name(),
                    r.role.to_string(),
                    rev.map(|r| short_rev(&r)).unwrap_or_else(|| "(default branch)".into()),
                    format!("{status}  {loc}"),
                ));
            }
            print_repo_table(&rows);
        }
        RepoCommand::Set {
            name,
            role,
            branch,
            rev,
            rename,
        } => {
            edit_config(ws_dir, |d| {
                if let Some(r) = role {
                    d.set_repo_key(&name, "role", Some(&r.to_string()))?;
                }
                if let Some(b) = &branch {
                    d.set_repo_key(&name, "branch", Some(b))?;
                }
                if let Some(r) = &rev {
                    d.set_repo_key(&name, "rev", if r == "none" { None } else { Some(r) })?;
                }
                if let Some(n) = &rename {
                    d.set_repo_key(&name, "name", Some(n))?;
                }
                Ok(())
            })?;
        }
        RepoCommand::Remove { name } => edit_config(ws_dir, |d| Ok(d.remove_repo(&name)?))?,
    }
    Ok(())
}

fn cmd_people(ws_dir: &Path, action: Option<PeopleCommand>, apply: bool) -> Result<()> {
    let ws = Workspace::load(ws_dir)?;
    let mut people = ws.config.people.clone();
    match action {
        Some(PeopleCommand::Merge { person, ids }) => {
            merge(&mut people, &person, &ids);
            return edit_config(ws_dir, |d| Ok(d.set_people(&people)?));
        }
        Some(PeopleCommand::Bot { ids }) => {
            add_unique(&mut people.bots, &ids);
            return edit_config(ws_dir, |d| Ok(d.set_people(&people)?));
        }
        Some(PeopleCommand::External { ids }) => {
            add_unique(&mut people.externals, &ids);
            return edit_config(ws_dir, |d| Ok(d.set_people(&people)?));
        }
        None => {}
    }
    let (idents, warnings) = gather(&ws);
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    let p = propose(&people, &idents);
    let mut current = String::new();
    for (i, s) in &p.rows {
        let (head, mark) = match s {
            Status::Mapped(person) => (person.clone(), " "),
            Status::Proposed(person) => (person.clone(), "+"),
            Status::Bot => ("bots (excluded)".into(), " "),
            Status::External => ("externals (excluded)".into(), " "),
        };
        if head != current {
            println!("{head}");
            current = head;
        }
        println!(
            "  {mark} {:<52} {:>6} commits  last {}  {}",
            i.label(),
            fmt_n(i.commits),
            i.last,
            i.repos.iter().cloned().collect::<Vec<_>>().join(", ")
        );
    }
    if !p.bot_hints.is_empty() {
        println!("\nlook automated (exclude with `pmx people bot <email>`):");
        for i in &p.bot_hints {
            println!("  {}", i.label());
        }
    }
    let n = p.changes();
    if n == 0 {
        println!("\n[people] already covers every identity.");
        return Ok(());
    }
    println!(
        "\n{n} identities marked + are not in [people] yet; above is where they would go.\n\
         Fix a grouping with `pmx people merge \"<Person>\" <email>…`, exclude with `pmx people bot|external`."
    );
    if apply || confirm("Write these into [people]?", false) {
        edit_config(ws_dir, |d| Ok(d.set_people(&p.people)?))?;
    } else {
        println!("nothing written (run `pmx people --apply` to write)");
    }
    Ok(())
}

/// A side file passed explicitly, or found next to the config or in `../tools/`.
fn side_file(explicit: Option<PathBuf>, config: &Path, name: &str) -> Result<Option<serde_json::Value>> {
    let dir = config.parent().unwrap_or(Path::new("."));
    let path = explicit.or_else(|| {
        [dir.join(name), dir.join("../tools").join(name)]
            .into_iter()
            .find(|p| p.exists())
    });
    match path {
        Some(p) => {
            eprintln!("using {}", p.display());
            let text = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
            Ok(Some(
                serde_json::from_str(&text).with_context(|| format!("parsing {}", p.display()))?,
            ))
        }
        None => Ok(None),
    }
}

fn cmd_import(
    ws: &Path,
    config: PathBuf,
    externals: Option<PathBuf>,
    fte: Option<PathBuf>,
    triage: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    let text = std::fs::read_to_string(&config).with_context(|| format!("reading {}", config.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", config.display()))?;
    let ext = side_file(externals, &config, "externals.json")?;
    let fte = side_file(fte, &config, "fte.json")?;
    let triage = side_file(triage, &config, "secrets_triage.json")?;
    let imported = from_prototype(
        &json,
        &SideFiles {
            externals: ext.as_ref(),
            fte: fte.as_ref(),
            secrets_triage: triage.as_ref(),
        },
    )?;
    for n in &imported.notes {
        eprintln!("note: {n}");
    }
    write_new_config(ws, &imported.config, force)?;
    println!("next: `pmx check`, then `pmx collect`");
    Ok(())
}

fn run_options(run: &RunArgs) -> CollectOptions {
    let mut opts = CollectOptions::new(run.as_of.unwrap_or_else(today));
    opts.fetch = !run.no_fetch;
    opts
}

fn cmd_plan(ws: &Workspace, opts: CollectOptions) -> Result<()> {
    let plans = plan(ws, &opts)?;
    plan_progress(ws, &opts, &plans)?;
    println!(
        "pmx plan · {} (as of {}{})",
        ws.config.project.name,
        opts.as_of,
        if opts.fetch { "; counted before fetch" } else { "" }
    );
    print!("{}", opts.progress.plan_report());
    println!("\nrepos:");
    let rows: Vec<_> = plans
        .iter()
        .zip(&ws.config.repos)
        .map(|(p, r)| {
            (
                p.name.clone(),
                r.role.to_string(),
                match (&p.rev, &p.tip) {
                    (Some(rev), Some(tip)) if short_rev(rev) == tip[..12] => format!("pinned @ {}", &tip[..12]),
                    (Some(rev), Some(tip)) => format!("{rev} @ {}", &tip[..12]),
                    _ => "-".into(),
                },
                format!(
                    "ingest: {} · rework: {}",
                    p.ingest.describe("commits"),
                    p.rework.describe("commits")
                ),
            )
        })
        .collect();
    print_repo_table(&rows);
    Ok(())
}

fn cmd_collect(ws: &Workspace, mut opts: CollectOptions, mode: Mode) -> Result<()> {
    let progress = Progress::new(mode, format!("pmx collect · {}", ws.config.project.name));
    opts.progress = progress.clone();
    let result = collect(ws, &opts).and_then(|c| {
        write_outputs(&ws.out_dir(), &c)?;
        Ok(c)
    });
    progress.end(result.is_ok());
    let c = result?;
    if mode == Mode::Tty && c.warnings.len() > 5 {
        for w in &c.warnings {
            eprintln!("warning: {w}");
        }
    }
    if mode != Mode::Json {
        eprintln!("wrote {}", ws.out_dir().join("project.json").display());
    }
    Ok(())
}

fn run(cli: Cli) -> Result<()> {
    let ws_dir = cli.workspace;
    match cli.command {
        Command::Init {
            dirs,
            name,
            since,
            yes,
            force,
        } => cmd_init(&ws_dir, dirs, name, since, yes, force)?,
        Command::Repo(cmd) => cmd_repo(&ws_dir, cmd)?,
        Command::People { action, apply } => cmd_people(&ws_dir, action, apply)?,
        Command::Check { threshold } => {
            let ws = Workspace::load(&ws_dir)?;
            let findings = pmx::check::check(&ws, threshold);
            if pmx::check::report(&findings, &mut std::io::stdout())? {
                bail!("check failed");
            }
        }
        Command::ImportConfig {
            config,
            externals,
            fte,
            secrets_triage,
            force,
        } => cmd_import(&ws_dir, config, externals, fte, secrets_triage, force)?,
        Command::Collect {
            run,
            dry_run,
            no_cache,
            full,
            progress,
        } => {
            let ws = Workspace::load(&ws_dir)?;
            let mut opts = run_options(&run);
            opts.use_cache = !no_cache;
            opts.incremental = !full;
            let mode: Mode = progress.parse().map_err(anyhow::Error::msg)?;
            if dry_run {
                cmd_plan(&ws, opts)?;
            } else {
                cmd_collect(&ws, opts, mode)?;
            }
        }
        Command::Plan { run } => {
            let ws = Workspace::load(&ws_dir)?;
            let opts = run_options(&run);
            cmd_plan(&ws, opts)?;
        }
        Command::Serve { port, open } => {
            let out = ws_dir.join("out");
            pmx::dashboard::render_workspace(&out, true)?;
            let listener = std::net::TcpListener::bind(("127.0.0.1", port))
                .with_context(|| format!("cannot listen on 127.0.0.1:{port} (try --port)"))?;
            let url = format!("http://{}/", listener.local_addr()?);
            eprintln!("serving {} at {url} (Ctrl-C to stop)", out.display());
            if !pmx::dashboard::has_engine() {
                eprintln!("note: this pmx was built without the WASM engine; custom ranges are off");
            }
            if open {
                pmx::dashboard::open_browser(&url);
            }
            pmx::dashboard::serve(listener, &out)?;
        }
        Command::Export {
            format: Format::Html,
            output,
            with_private,
            ..
        } => {
            let out = ws_dir.join("out");
            let html = pmx::dashboard::render_workspace(&out, with_private)?;
            let dest = output.unwrap_or_else(|| {
                out.join(if with_private {
                    "private/dashboard.html"
                } else {
                    "dashboard.html"
                })
            });
            if let Some(dir) = dest.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&dest, html).with_context(|| format!("writing {}", dest.display()))?;
            eprintln!(
                "wrote {}{}",
                dest.display(),
                if with_private {
                    " (contains per-person data: lead only, don't share)"
                } else {
                    ""
                }
            );
        }
        Command::Export {
            format,
            weekly,
            monthly,
            ..
        } => {
            let path = ws_dir.join("out").join("project.json");
            let text = std::fs::read_to_string(&path)
                .with_context(|| format!("reading {} (run `pmx collect` first)", path.display()))?;
            let p: ProjectFile = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
            if weekly && monthly {
                bail!("choose --weekly or --monthly");
            }
            let cadence = if weekly { Cadence::Weekly } else { Cadence::Monthly };
            match format {
                Format::Long => print!("{}", to_csv(&long_rows(&p, cadence))),
                Format::Html => unreachable!("handled above"),
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
