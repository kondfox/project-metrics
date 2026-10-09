//! Progress and ETA for `pmx collect` (plan §5).
//!
//! Work is split into stages (fetch, git ingest, rework walk, …), each with a number of units
//! (repos, commits). `ETA = Σ remaining units × cost per unit`: the cost starts from what earlier
//! runs measured (or a built-in default) and is corrected live from the stage's observed rate.
//! Renderers: a redrawn block on a terminal, a status line every 10 s for pipes and CI, or
//! newline-delimited JSON events.

use std::fmt::Write as _;
use std::io::{IsTerminal, Write};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Nothing is printed.
    Off,
    /// A block redrawn in place (stderr is a terminal).
    Tty,
    /// A status line every 10 seconds (CI, pipes).
    Plain,
    /// Newline-delimited JSON events on stdout.
    Json,
}

impl Mode {
    /// `Tty` when stderr is a terminal, else `Plain`.
    pub fn auto() -> Mode {
        if std::io::stderr().is_terminal() {
            Mode::Tty
        } else {
            Mode::Plain
        }
    }
}

impl FromStr for Mode {
    type Err = String;
    fn from_str(s: &str) -> Result<Mode, String> {
        match s {
            "auto" => Ok(Mode::auto()),
            "tty" => Ok(Mode::Tty),
            "plain" => Ok(Mode::Plain),
            "json" => Ok(Mode::Json),
            "off" | "none" => Ok(Mode::Off),
            _ => Err(format!("unknown progress mode `{s}` (auto, tty, plain, json, off)")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Pending,
    Running,
    Done,
    Skipped,
}

#[derive(Clone, Debug)]
pub struct Stage {
    pub id: &'static str,
    pub label: &'static str,
    /// Unit name, plural (`commits`, `repos`).
    pub unit: &'static str,
    /// Units to process in this run.
    pub total: u64,
    pub done: u64,
    /// Units served from the cache (reported, not processed).
    pub cached: u64,
    /// Seconds per unit for one worker, from earlier runs or a default.
    pub prior_cost: f64,
    /// `total` is an estimate.
    pub estimated: bool,
    pub status: Status,
    pub detail: String,
    /// Units processed at the same time in this stage, if not the run's worker count.
    pub workers: Option<usize>,
    started: Option<Instant>,
    elapsed: Option<Duration>,
}

impl Stage {
    fn elapsed(&self) -> Duration {
        match (self.elapsed, self.started) {
            (Some(e), _) => e,
            (None, Some(s)) => s.elapsed(),
            _ => Duration::ZERO,
        }
    }
}

struct State {
    stages: Vec<Stage>,
    warnings: Vec<String>,
    workers: usize,
    lines_drawn: usize,
}

struct Inner {
    mode: Mode,
    title: String,
    start: Instant,
    state: Mutex<State>,
    stop: AtomicBool,
    renderer: Mutex<Option<JoinHandle<()>>>,
}

/// Shared progress handle; cheap to clone, safe to use from worker threads.
#[derive(Clone)]
pub struct Progress(Arc<Inner>);

impl Progress {
    pub fn new(mode: Mode, title: impl Into<String>) -> Progress {
        Progress(Arc::new(Inner {
            mode,
            title: title.into(),
            start: Instant::now(),
            state: Mutex::new(State {
                stages: Vec::new(),
                warnings: Vec::new(),
                workers: 1,
                lines_drawn: 0,
            }),
            stop: AtomicBool::new(false),
            renderer: Mutex::new(None),
        }))
    }

    /// A handle that tracks but never prints.
    pub fn off() -> Progress {
        Progress::new(Mode::Off, "")
    }

    pub fn mode(&self) -> Mode {
        self.0.mode
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.0.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn with_stage(&self, id: &str, f: impl FnOnce(&mut Stage)) {
        let mut st = self.state();
        if let Some(s) = st.stages.iter_mut().find(|s| s.id == id) {
            f(s);
        }
    }

    pub fn add_stage(&self, id: &'static str, label: &'static str, unit: &'static str, prior_cost: f64) {
        self.state().stages.push(Stage {
            id,
            label,
            unit,
            total: 0,
            done: 0,
            cached: 0,
            prior_cost,
            estimated: false,
            status: Status::Pending,
            detail: String::new(),
            workers: None,
            started: None,
            elapsed: None,
        });
    }

    /// This stage runs `n` units at a time (e.g. bounded snapshot scans).
    pub fn set_stage_workers(&self, id: &str, n: usize) {
        self.with_stage(id, |s| s.workers = Some(n.max(1)));
    }

    /// Set the work of a stage (before or while it runs).
    pub fn plan(&self, id: &str, total: u64, cached: u64, estimated: bool) {
        self.with_stage(id, |s| {
            s.total = total;
            s.cached = cached;
            s.estimated = estimated;
        });
    }

    /// Repos processed at the same time.
    pub fn set_workers(&self, n: usize) {
        self.state().workers = n.max(1);
    }

    pub fn start(&self, id: &str) {
        self.with_stage(id, |s| {
            if s.status == Status::Pending {
                s.status = Status::Running;
                s.started = Some(Instant::now());
            }
        });
    }

    pub fn advance(&self, id: &str, n: u64) {
        self.with_stage(id, |s| {
            s.done += n;
            // An estimated total grows when the real work turns out larger.
            if s.done > s.total {
                s.total = s.done;
            }
        });
    }

    pub fn detail(&self, id: &str, text: impl Into<String>) {
        let text = text.into();
        self.with_stage(id, |s| s.detail = text);
    }

    pub fn finish(&self, id: &str) {
        let mut ev = None;
        self.with_stage(id, |s| {
            if s.status == Status::Running || s.status == Status::Pending {
                s.status = Status::Done;
                s.elapsed = Some(s.elapsed());
                s.total = s.done;
                s.estimated = false;
                s.detail.clear();
                ev = Some(s.clone());
            }
        });
        if let Some(s) = ev {
            match self.0.mode {
                Mode::Plain => eprintln!("{}", done_line(&s)),
                Mode::Json => self.emit(json!({
                    "event": "stage_done", "stage": s.id, "units": s.done, "cached": s.cached,
                    "elapsed_s": secs(s.elapsed()),
                })),
                _ => {}
            }
        }
    }

    pub fn skip(&self, id: &str, why: impl Into<String>) {
        let why = why.into();
        self.with_stage(id, |s| {
            s.status = Status::Skipped;
            s.detail = why;
        });
    }

    pub fn warn(&self, msg: impl Into<String>) {
        let msg = msg.into();
        match self.0.mode {
            Mode::Plain => eprintln!("warning: {msg}"),
            Mode::Json => self.emit(json!({"event": "warning", "message": msg})),
            _ => {}
        }
        self.state().warnings.push(msg);
    }

    pub fn warnings(&self) -> Vec<String> {
        self.state().warnings.clone()
    }

    pub fn stages(&self) -> Vec<Stage> {
        self.state().stages.clone()
    }

    /// Seconds left, all stages.
    pub fn eta(&self) -> f64 {
        let st = self.state();
        eta(&st.stages, st.workers)
    }

    fn emit(&self, v: serde_json::Value) {
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{v}");
        let _ = out.flush();
    }

    /// Start the renderer thread. Call after the stages are planned.
    pub fn begin(&self) {
        match self.0.mode {
            Mode::Off => return,
            Mode::Json => {
                let st = self.state();
                let stages: Vec<_> = st
                    .stages
                    .iter()
                    .map(|s| json!({"stage": s.id, "units": s.total, "cached": s.cached, "estimated": s.estimated}))
                    .collect();
                let eta_s = secs(Duration::from_secs_f64(eta(&st.stages, st.workers)));
                drop(st);
                self.emit(json!({"event": "plan", "title": self.0.title, "stages": stages, "eta_s": eta_s}));
            }
            _ => {}
        }
        let me = self.clone();
        let handle = std::thread::spawn(move || {
            let tick = match me.0.mode {
                Mode::Tty => Duration::from_millis(100),
                Mode::Json => Duration::from_secs(1),
                _ => Duration::from_secs(10),
            };
            let mut last = Instant::now();
            while !me.0.stop.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(50));
                if last.elapsed() >= tick {
                    last = Instant::now();
                    me.render();
                }
            }
        });
        *self.0.renderer.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);
    }

    fn render(&self) {
        match self.0.mode {
            Mode::Tty => {
                let mut st = self.state();
                let lines = tty_block(
                    &self.0.title,
                    &st.stages,
                    &st.warnings,
                    st.workers,
                    self.0.start.elapsed(),
                );
                let mut err = std::io::stderr().lock();
                let mut buf = String::new();
                if st.lines_drawn > 0 {
                    let _ = write!(buf, "\x1b[{}A", st.lines_drawn);
                }
                for l in &lines {
                    let _ = writeln!(buf, "\r\x1b[2K{l}");
                }
                let _ = err.write_all(buf.as_bytes());
                let _ = err.flush();
                st.lines_drawn = lines.len();
            }
            Mode::Plain => {
                let st = self.state();
                eprintln!("{}", plain_line(&st.stages, st.workers, self.0.start.elapsed()));
            }
            Mode::Json => {
                let st = self.state();
                for s in st.stages.iter().filter(|s| s.status == Status::Running) {
                    self.emit(json!({
                        "event": "progress", "stage": s.id, "done": s.done, "total": s.total,
                        "detail": s.detail, "eta_s": secs(Duration::from_secs_f64(eta(&st.stages, st.workers))),
                    }));
                }
            }
            Mode::Off => {}
        }
    }

    /// Stop the renderer, draw the final state and print the summary.
    pub fn end(&self, ok: bool) {
        self.0.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.0.renderer.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = h.join();
        }
        let elapsed = self.0.start.elapsed();
        match self.0.mode {
            Mode::Off => {}
            Mode::Json => {
                let st = self.state();
                let stages: Vec<_> = st
                    .stages
                    .iter()
                    .map(|s| json!({"stage": s.id, "units": s.done, "cached": s.cached, "status": format!("{:?}", s.status).to_lowercase()}))
                    .collect();
                let warnings = st.warnings.clone();
                drop(st);
                self.emit(json!({"event": "done", "ok": ok, "elapsed_s": secs(elapsed), "stages": stages, "warnings": warnings}));
            }
            Mode::Tty => {
                self.render();
                eprintln!("{}", summary(&self.state().stages, ok, elapsed));
            }
            Mode::Plain => {
                let st = self.state();
                eprintln!("{}", summary(&st.stages, ok, elapsed));
            }
        }
    }

    /// The `--dry-run` / `pmx plan` table.
    pub fn plan_report(&self) -> String {
        let st = self.state();
        let mut out = String::new();
        let _ = writeln!(out, "{:<14} {:<34} {:>9}", "stage", "work", "estimate");
        let mut total = 0.0;
        for s in &st.stages {
            let secs = remaining(s, st.workers, false);
            total += secs;
            let est = if s.status == Status::Skipped {
                "-".to_string()
            } else {
                format!("~{}", fmt_dur(Duration::from_secs_f64(secs)))
            };
            let _ = writeln!(out, "{:<14} {:<34} {:>9}", s.label, work_text(s), est);
        }
        let _ = writeln!(
            out,
            "{:<14} {:<34} {:>9}",
            "",
            "total",
            format!("~{}", fmt_dur(Duration::from_secs_f64(total)))
        );
        out
    }
}

fn secs(d: Duration) -> f64 {
    (d.as_secs_f64() * 10.0).round() / 10.0
}

/// Seconds left in one stage. Before it runs: prior cost spread over the workers. While it runs,
/// the observed wall-clock rate takes over in proportion to the work done.
fn remaining(s: &Stage, workers: usize, live: bool) -> f64 {
    if matches!(s.status, Status::Done | Status::Skipped) {
        return 0.0;
    }
    let left = s.total.saturating_sub(s.done) as f64;
    let prior = s.prior_cost / s.workers.unwrap_or(workers).max(1) as f64;
    let el = s.elapsed().as_secs_f64();
    let cost = if live && s.status == Status::Running && s.done > 0 && el >= 1.0 && s.total > 0 {
        let w = (s.done as f64 / s.total as f64).min(1.0);
        prior * (1.0 - w) + (el / s.done as f64) * w
    } else {
        prior
    };
    left * cost
}

pub fn eta(stages: &[Stage], workers: usize) -> f64 {
    stages.iter().map(|s| remaining(s, workers, true)).sum()
}

/// Share of the planned work that is done, weighted by cost.
fn overall(stages: &[Stage]) -> f64 {
    let (mut done, mut total) = (0.0, 0.0);
    for s in stages.iter().filter(|s| s.status != Status::Skipped) {
        let w = s.prior_cost.max(1e-6);
        total += s.total as f64 * w;
        done += s.done.min(s.total) as f64 * w;
    }
    if total == 0.0 { 1.0 } else { done / total }
}

pub fn fmt_dur(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m", (s + 30) / 60)
    } else {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    }
}

pub fn fmt_n(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn bar(frac: f64, width: usize) -> String {
    let filled = ((frac.clamp(0.0, 1.0)) * width as f64).round() as usize;
    format!("▕{}{}▏", "█".repeat(filled), "░".repeat(width - filled))
}

fn work_text(s: &Stage) -> String {
    let approx = if s.estimated { "~" } else { "" };
    let cached = if s.cached > 0 {
        format!(" ({} cached)", fmt_n(s.cached))
    } else {
        String::new()
    };
    match s.status {
        Status::Skipped => s.detail.clone(),
        _ => format!("{approx}{} {}{cached}", fmt_n(s.total), s.unit),
    }
}

fn done_line(s: &Stage) -> String {
    format!(" ✓ {:<14} {:<40} {}", s.label, work_text(s), fmt_dur(s.elapsed()))
}

fn tty_block(title: &str, stages: &[Stage], warnings: &[String], workers: usize, elapsed: Duration) -> Vec<String> {
    let mut lines = Vec::new();
    let frac = overall(stages);
    let eta_s = eta(stages, workers);
    lines.push(format!(
        "{title:<44} overall {} {:>3}%  ETA ~{}  ({})",
        bar(frac, 16),
        (frac * 100.0).floor() as u64,
        fmt_dur(Duration::from_secs_f64(eta_s)),
        fmt_dur(elapsed)
    ));
    for s in stages {
        let line = match s.status {
            Status::Done => done_line(s),
            Status::Skipped => format!(" - {:<14} {}", s.label, s.detail),
            Status::Pending => format!(
                " ○ {:<14} {:<40} ~{}",
                s.label,
                work_text(s),
                fmt_dur(Duration::from_secs_f64(remaining(s, workers, true)))
            ),
            Status::Running => {
                let frac = if s.total == 0 {
                    0.0
                } else {
                    s.done as f64 / s.total as f64
                };
                format!(
                    " ● {:<14} {} {}/{} {:<24} ~{}",
                    s.label,
                    bar(frac, 10),
                    fmt_n(s.done),
                    fmt_n(s.total),
                    truncate(&s.detail, 24),
                    fmt_dur(Duration::from_secs_f64(remaining(s, workers, true)))
                )
            }
        };
        lines.push(line);
    }
    for w in warnings.iter().rev().take(5).rev() {
        lines.push(format!(" ⚠ {}", truncate(w, 100)));
    }
    lines
}

fn plain_line(stages: &[Stage], workers: usize, elapsed: Duration) -> String {
    let parts: Vec<String> = stages
        .iter()
        .filter(|s| s.status == Status::Running)
        .map(|s| format!("{} {}/{}", s.label, fmt_n(s.done), fmt_n(s.total)))
        .collect();
    format!(
        "[{}] {} · ETA ~{}",
        fmt_dur(elapsed),
        if parts.is_empty() {
            "working".to_string()
        } else {
            parts.join(" · ")
        },
        fmt_dur(Duration::from_secs_f64(eta(stages, workers)))
    )
}

fn summary(stages: &[Stage], ok: bool, elapsed: Duration) -> String {
    let parts: Vec<String> = stages
        .iter()
        .filter(|s| s.status == Status::Done)
        .map(|s| format!("{} {}", s.label, work_text(s)))
        .collect();
    format!(
        "{} in {} · {}",
        if ok { "done" } else { "stopped" },
        fmt_dur(elapsed),
        parts.join(" · ")
    )
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage(total: u64, done: u64, cost: f64, status: Status) -> Stage {
        Stage {
            id: "x",
            label: "x",
            unit: "commits",
            total,
            done,
            cached: 0,
            prior_cost: cost,
            estimated: false,
            status,
            detail: String::new(),
            workers: None,
            started: None,
            elapsed: None,
        }
    }

    #[test]
    fn eta_from_prior_costs() {
        let stages = [
            stage(1000, 0, 0.002, Status::Pending),
            stage(10, 10, 1.0, Status::Done),
            stage(500, 0, 0.004, Status::Skipped),
        ];
        assert!((eta(&stages, 1) - 2.0).abs() < 1e-9);
        // Two workers halve it.
        assert!((eta(&stages, 2) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn overall_share() {
        let stages = [
            stage(100, 50, 1.0, Status::Running),
            stage(100, 0, 1.0, Status::Pending),
        ];
        assert!((overall(&stages) - 0.25).abs() < 1e-9);
        assert_eq!(overall(&[]), 1.0);
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_n(12418), "12,418");
        assert_eq!(fmt_n(999), "999");
        assert_eq!(fmt_n(1_000_000), "1,000,000");
        assert_eq!(fmt_dur(Duration::from_secs(4)), "4s");
        assert_eq!(fmt_dur(Duration::from_secs(359)), "6m");
        assert_eq!(fmt_dur(Duration::from_secs(3720)), "1h 2m");
        assert_eq!(bar(0.5, 4), "▕██░░▏");
        assert_eq!(truncate("abcdef", 4), "abc…");
    }

    #[test]
    fn tracks_stages() {
        let p = Progress::off();
        p.add_stage("ingest", "git ingest", "commits", 0.001);
        p.plan("ingest", 100, 40, true);
        p.start("ingest");
        p.advance("ingest", 150);
        p.finish("ingest");
        let s = &p.stages()[0];
        assert_eq!((s.total, s.done, s.cached, s.status), (150, 150, 40, Status::Done));
        p.warn("semgrep missing");
        assert_eq!(p.warnings(), ["semgrep missing"]);
        assert!(p.plan_report().contains("git ingest"));
        p.end(true);
    }

    #[test]
    fn modes_parse() {
        assert_eq!("json".parse::<Mode>().unwrap(), Mode::Json);
        assert!("loud".parse::<Mode>().is_err());
    }
}
