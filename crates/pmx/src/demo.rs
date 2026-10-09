//! `pmx demo`: a fictional project to try the tool and to develop the dashboard against, with
//! nothing real in it. Four repos, five people over ~14 months, written with `git fast-import`.
//! The same `as_of` always gives the same history.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Days, NaiveDate};
use pm_classify::Role;
use pm_config::{Config, People, Project, RepoConfig, RepoRole, Workspace};

/// A small deterministic generator (SplitMix64).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
    fn chance(&mut self, p: f64) -> bool {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64 <= p
    }
}

struct RepoSpec {
    name: &'static str,
    role: Role,
    dir: &'static str,
    exts: &'static [&'static str],
    test: fn(&str, &str) -> String,
    lockfile: &'static str,
}

const REPOS: &[RepoSpec] = &[
    RepoSpec {
        name: "shop-api",
        role: Role::Backend,
        dir: "internal",
        exts: &["go", "go", "go", "sql"],
        test: |dir, stem| format!("{dir}/{stem}_test.go"),
        lockfile: "go.mod",
    },
    RepoSpec {
        name: "shop-web",
        role: Role::Frontend,
        dir: "src",
        exts: &["tsx", "tsx", "ts", "css"],
        test: |dir, stem| format!("{dir}/{stem}.test.tsx"),
        lockfile: "package-lock.json",
    },
    RepoSpec {
        name: "shop-mobile",
        role: Role::Mobile,
        dir: "app/src/main/kotlin/shop",
        exts: &["kt", "kt", "kts"],
        test: |_, stem| format!("app/src/test/kotlin/shop/{}Test.kt", capitalize(stem)),
        lockfile: "gradle.lockfile",
    },
    RepoSpec {
        name: "shop-e2e",
        role: Role::Qa,
        dir: "e2e",
        exts: &["ts"],
        test: |dir, stem| format!("{dir}/{stem}.spec.ts"),
        lockfile: "package-lock.json",
    },
];

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

struct Person {
    name: &'static str,
    emails: &'static [&'static str],
    /// Weight per repo in `REPOS` order.
    repos: [u32; 4],
    weekly: u64,
    tests: f64,
    docs: f64,
    /// Months after the start before the person's first commit.
    joins: u32,
}

const PEOPLE: &[Person] = &[
    Person {
        name: "Ana Kovacs",
        emails: &["ana@acme.example"],
        repos: [4, 6, 0, 0],
        weekly: 4,
        tests: 0.5,
        docs: 0.15,
        joins: 0,
    },
    Person {
        name: "Ben Ortiz",
        emails: &["ben@acme.example", "ben.ortiz@home.example"],
        repos: [9, 0, 0, 1],
        weekly: 4,
        tests: 0.6,
        docs: 0.25,
        joins: 0,
    },
    Person {
        name: "Chen Wu",
        emails: &["chen@acme.example"],
        repos: [3, 0, 7, 0],
        weekly: 3,
        tests: 0.3,
        docs: 0.05,
        joins: 0,
    },
    Person {
        name: "Dana Fox",
        emails: &["dana@acme.example"],
        repos: [0, 4, 0, 6],
        weekly: 3,
        tests: 0.2,
        docs: 0.1,
        joins: 0,
    },
    Person {
        name: "Eli Novak",
        emails: &["eli@acme.example"],
        repos: [0, 10, 0, 0],
        weekly: 3,
        tests: 0.25,
        docs: 0.05,
        joins: 4,
    },
];

const EXTERNAL: (&str, &str) = ("Sam Contractor", "sam@agency.example");
const BOT: (&str, &str) = ("dependabot[bot]", "49699333+dependabot[bot]@users.noreply.github.com");

#[derive(Default)]
struct RepoState {
    files: BTreeMap<String, Vec<String>>,
    stream: Vec<u8>,
    counter: u64,
    last_touched: Option<String>,
}

impl RepoState {
    fn line(&mut self, ext: &str) -> String {
        self.counter += 1;
        let n = self.counter;
        // Every fourth line branches, so complexity and hotspots have something to measure.
        if n % 4 == 0 {
            return match ext {
                "go" => format!("\tif v{n} > {} {{ return v{n} }}", n % 13),
                "kt" | "kts" => format!("    if (v{n} > {}) return v{n}", n % 13),
                "ts" | "tsx" => format!("if (v{n} > {} && ready) return v{n};", n % 13),
                _ => self.line_plain(ext, n),
            };
        }
        self.line_plain(ext, n)
    }

    fn line_plain(&self, ext: &str, n: u64) -> String {
        match ext {
            "go" => format!("\tv{n} := compute{}(ctx, {n})", n % 97),
            "sql" => format!("ALTER TABLE orders ADD COLUMN c{n} integer DEFAULT {n};"),
            "kt" | "kts" => format!("    val v{n} = compute{}({n})", n % 97),
            "css" => format!(".item-{n} {{ margin: {}px; }}", n % 40),
            "md" => format!("- Note {n}: behaviour of step {} explained.", n % 50),
            _ => format!("export const v{n} = compute{}({n});", n % 97),
        }
    }

    fn commit(&mut self, who: (&str, &str), when: i64, msg: &str, changes: &[String], deleted: &[String]) {
        let mut s = String::new();
        s.push_str("commit refs/heads/main\n");
        for role in ["author", "committer"] {
            s.push_str(&format!("{role} {} <{}> {when} +0100\n", who.0, who.1));
        }
        s.push_str(&format!("data {}\n{msg}\n", msg.len()));
        let mut out = s.into_bytes();
        for path in changes {
            let body = self.files[path].join("\n") + "\n";
            out.extend_from_slice(format!("M 100644 inline {path}\ndata {}\n", body.len()).as_bytes());
            out.extend_from_slice(body.as_bytes());
            out.push(b'\n');
        }
        for path in deleted {
            out.extend_from_slice(format!("D {path}\n").as_bytes());
        }
        out.push(b'\n');
        self.stream.extend(out);
    }
}

/// Real lockfile formats with old versions of real packages that have public advisories, upgraded
/// in stages (the demo's Security trend improves).
fn lockfile(repo: &str, stage: usize) -> Vec<String> {
    let pick = |versions: &[&'static str]| versions[stage.min(versions.len() - 1)];
    let npm = |deps: &[(&str, &str)]| -> Vec<String> {
        let mut out = vec![
            "{".to_string(),
            format!("  \"name\": \"{repo}\","),
            "  \"version\": \"1.0.0\",".into(),
            "  \"lockfileVersion\": 3,".into(),
            "  \"requires\": true,".into(),
            "  \"packages\": {".into(),
            format!("    \"\": {{ \"name\": \"{repo}\", \"version\": \"1.0.0\" }},"),
        ];
        for (i, (name, v)) in deps.iter().enumerate() {
            let comma = if i + 1 < deps.len() { "," } else { "" };
            out.push(format!(
                "    \"node_modules/{name}\": {{ \"version\": \"{v}\" }}{comma}"
            ));
        }
        out.extend(["  }".to_string(), "}".into()]);
        out
    };
    match repo {
        "shop-api" => vec![
            "module example.com/shop-api".into(),
            String::new(),
            "go 1.21".into(),
            String::new(),
            "require (".into(),
            format!("\tgolang.org/x/net {}", pick(&["v0.7.0", "v0.17.0", "v0.33.0"])),
            format!(
                "\tgolang.org/x/crypto {}",
                pick(&["v0.0.0-20200622213623-75b288015ac9", "v0.17.0", "v0.31.0"])
            ),
            ")".into(),
        ],
        "shop-mobile" => vec![
            "# This is a Gradle generated file for dependency locking.".into(),
            format!(
                "com.squareup.okhttp3:okhttp:{}=releaseRuntimeClasspath",
                pick(&["3.12.0", "4.9.0", "4.12.0"])
            ),
            format!(
                "com.fasterxml.jackson.core:jackson-databind:{}=releaseRuntimeClasspath",
                pick(&["2.9.8", "2.12.7.1", "2.17.2"])
            ),
            "empty=".into(),
        ],
        "shop-e2e" => npm(&[("ws", pick(&["7.4.5", "7.5.10", "8.17.1"]))]),
        _ => npm(&[
            ("lodash", pick(&["4.17.15", "4.17.20", "4.17.21"])),
            ("axios", pick(&["0.21.0", "0.21.1", "0.27.2", "1.7.4"])),
            ("minimist", pick(&["1.2.0", "1.2.5", "1.2.8"])),
        ]),
    }
}

fn timestamp(day: NaiveDate, hour: u32) -> i64 {
    day.and_hms_opt(hour, 17, 0).expect("valid time").and_utc().timestamp() - 3600
}

fn month_add(d: NaiveDate, months: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).expect("valid month")
}

/// Write the demo repos and `pmx.toml` into `dir` (which must be empty or missing).
pub fn generate(dir: &Path, as_of: NaiveDate) -> Result<Workspace> {
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        bail!("{} is not empty", dir.display());
    }
    let range_start = month_add(as_of, -13);
    let history_start = month_add(range_start, -2);
    let mut rng = Rng(as_of.num_days_from_ce() as u64);
    let mut repos: Vec<RepoState> = REPOS.iter().map(|_| RepoState::default()).collect();
    let total_days = (as_of - history_start).num_days().max(1) as f64;

    // Every repo starts with a README and a lockfile.
    for (r, spec) in repos.iter_mut().zip(REPOS) {
        r.files.insert(
            "README.md".into(),
            vec![format!("# {}", spec.name), "Fictional demo repo.".into()],
        );
        r.files.insert(spec.lockfile.into(), lockfile(spec.name, 0));
        let paths = vec!["README.md".to_string(), spec.lockfile.to_string()];
        r.commit(
            PEOPLE[1].emails.first().map(|e| (PEOPLE[1].name, *e)).unwrap(),
            timestamp(history_start, 9),
            "init",
            &paths,
            &[],
        );
    }

    let mut ticket = 100;
    let mut week = history_start - Days::new(history_start.weekday().num_days_from_monday() as u64);
    while week <= as_of {
        let t = (week - history_start).num_days() as f64 / total_days;
        // Collect the week's commits, then write them in time order.
        let mut events: Vec<(i64, usize, usize)> = Vec::new();
        for (pi, p) in PEOPLE.iter().enumerate() {
            if week < month_add(history_start, p.joins as i32) {
                continue;
            }
            let n = rng.below(p.weekly * 2 + 1);
            for _ in 0..n {
                let day = week + Days::new(rng.below(5));
                if day > as_of {
                    continue;
                }
                let total: u32 = p.repos.iter().sum();
                let mut pick = rng.below(total as u64) as u32;
                let ri = p.repos.iter().position(|w| {
                    if pick < *w {
                        true
                    } else {
                        pick -= w;
                        false
                    }
                });
                events.push((timestamp(day, 9 + rng.below(9) as u32), pi, ri.unwrap_or(0)));
            }
        }
        events.sort();
        for (when, pi, ri) in events {
            let p = &PEOPLE[pi];
            let spec = &REPOS[ri];
            let r = &mut repos[ri];
            ticket += 1;
            let email = p.emails[if p.emails.len() > 1 && rng.chance(0.2) { 1 } else { 0 }];
            let ext = spec.exts[rng.below(spec.exts.len() as u64) as usize];
            let mut changed = Vec::new();

            // Rework: replace part of the file touched last, often within days.
            if let Some(last) = r.last_touched.clone().filter(|_| rng.chance(0.35)) {
                if let Some(lines) = r.files.get(&last).cloned() {
                    let keep = lines.len() - (lines.len() * (1 + rng.below(4) as usize) / 10);
                    let fresh: Vec<String> = (0..rng.below(6)).map(|_| r.line(ext)).collect();
                    r.files
                        .insert(last.clone(), lines[..keep].iter().cloned().chain(fresh).collect());
                    changed.push(last);
                }
            }
            // New work: a new file or more lines in an existing one.
            let stem = format!("feature{}", rng.below(40));
            let path = format!("{}/{stem}.{ext}", spec.dir);
            let add = 5 + rng.below(70);
            let lines: Vec<String> = (0..add).map(|_| r.line(ext)).collect();
            r.files.entry(path.clone()).or_default().extend(lines);
            changed.push(path.clone());
            // Now and then someone copies a block instead of extracting it (duplication).
            if rng.chance(0.08) {
                let block: Vec<String> = r.files[&path].iter().take(12).cloned().collect();
                if block.len() >= 8 {
                    let copy = format!("{}/legacy{}.{ext}", spec.dir, rng.below(1000));
                    r.files.entry(copy.clone()).or_default().extend(block);
                    changed.push(copy);
                }
            }
            r.last_touched = Some(path);
            // Tests and docs with the code; habits improve over time.
            if rng.chance(p.tests * (0.6 + 0.6 * t)) {
                let tpath = (spec.test)(spec.dir, &stem);
                let text: Vec<String> = (0..5 + rng.below(25)).map(|_| r.line(ext)).collect();
                r.files.entry(tpath.clone()).or_default().extend(text);
                changed.push(tpath);
            }
            if rng.chance(p.docs) {
                let d = if rng.chance(0.5) {
                    "README.md".to_string()
                } else {
                    "docs/notes.md".to_string()
                };
                let note = r.line("md");
                r.files.entry(d.clone()).or_default().push(note);
                changed.push(d);
            }
            changed.sort();
            changed.dedup();
            let ai = rng.chance(0.75 * t * t);
            let msg = format!(
                "ACME-{ticket} {} {}{}",
                ["Add", "Fix", "Refactor", "Improve", "Extend"][rng.below(5) as usize],
                spec.name.trim_start_matches("shop-"),
                if ai {
                    "\n\nCo-Authored-By: Claude <noreply@anthropic.com>"
                } else {
                    ""
                }
            );
            r.commit((p.name, email), when, &msg, &changed, &[]);
        }
        // Monthly dependency bumps by a bot; an external contractor for a few months.
        // Dependency upgrades every few months, so known advisories get fixed over time.
        let stage = ((week - history_start).num_days() / 120) as usize;
        if week.day() <= 7 {
            for (r, spec) in repos.iter_mut().zip(REPOS) {
                let next = lockfile(spec.name, stage);
                if r.files.get(spec.lockfile) == Some(&next) {
                    continue;
                }
                r.files.insert(spec.lockfile.into(), next);
                r.commit(
                    BOT,
                    timestamp(week, 6),
                    "Bump dependencies",
                    &[spec.lockfile.to_string()],
                    &[],
                );
            }
        }
        let ext_from = month_add(history_start, 4);
        if week >= ext_from && week < month_add(ext_from, 3) {
            let r = &mut repos[1];
            let path = "src/legacy/Checkout.tsx".to_string();
            let lines: Vec<String> = (0..40).map(|_| r.line("tsx")).collect();
            r.files.entry(path.clone()).or_default().extend(lines);
            r.commit(
                EXTERNAL,
                timestamp(week + Days::new(2), 14),
                "Checkout rework",
                &[path],
                &[],
            );
        }
        week = week + Days::new(7);
    }

    std::fs::create_dir_all(dir.join("repos"))?;
    for (r, spec) in repos.iter().zip(REPOS) {
        let path = dir.join("repos").join(spec.name);
        let git = |args: &[&str]| -> Result<()> {
            let ok = Command::new("git").arg("-C").arg(&path).args(args).status()?.success();
            if !ok {
                bail!("git {args:?} failed in {}", path.display());
            }
            Ok(())
        };
        std::fs::create_dir_all(&path)?;
        git(&["init", "-q", "-b", "main"])?;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["fast-import", "--quiet", "--done"])
            .stdin(Stdio::piped())
            .spawn()
            .context("running git fast-import")?;
        let mut stdin = child.stdin.take().expect("piped");
        stdin.write_all(&r.stream)?;
        stdin.write_all(b"done\n")?;
        drop(stdin);
        if !child.wait()?.success() {
            bail!("git fast-import failed for {}", spec.name);
        }
        git(&["reset", "-q", "--hard", "main"])?;
    }

    let mut people = People::default();
    for p in PEOPLE {
        people
            .persons
            .insert(p.name.into(), p.emails.iter().map(|e| e.to_string()).collect());
    }
    people.externals = vec![EXTERNAL.0.into()];
    let config = Config {
        project: Project {
            name: "Acme Shop (demo)".into(),
            range_start,
            breadth_roles: vec![Role::Frontend, Role::Backend, Role::Mobile, Role::Qa],
            ai_attribution: true,
        },
        repos: REPOS
            .iter()
            .map(|s| RepoConfig {
                path: Some(format!("repos/{}", s.name)),
                branch: Some("main".into()),
                role: RepoRole::Fixed(s.role),
                ..Default::default()
            })
            .collect(),
        role_rules: None,
        code_host: None,
        people,
        fte: None,
        secrets_triage: Vec::new(),
        snapshots: Default::default(),
        classifiers: BTreeMap::new(),
        velocity: None,
    };
    config.validate()?;
    std::fs::write(
        dir.join(pm_config::CONFIG_FILE),
        format!("# A fictional demo project made by `pmx demo`.\n\n{}", config.to_toml()),
    )?;
    Ok(Workspace {
        root: dir.to_path_buf(),
        config,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_rng() {
        let mut a = Rng(7);
        let mut b = Rng(7);
        assert_eq!(
            (0..5).map(|_| a.next()).collect::<Vec<_>>(),
            (0..5).map(|_| b.next()).collect::<Vec<_>>()
        );
        assert!((0..1000).all(|_| a.below(10) < 10));
    }

    #[test]
    fn months() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 9).unwrap();
        assert_eq!(month_add(d, -13), NaiveDate::from_ymd_opt(2025, 9, 1).unwrap());
        assert_eq!(month_add(d, 3), NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }
}
