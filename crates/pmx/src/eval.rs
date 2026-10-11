//! `pmx classifier eval` (plan §6.5): score a reference set's work packages with a candidate
//! classifier and compare with the reference scores: per-axis agreement (quadratic-weighted κ),
//! points, project Velocity drift, test–retest stability and the share of unsure answers.
//!
//! The reference set holds only ids and scores (`project,id,R1..R6`); each package's content is
//! rebuilt from the workspace's repos exactly as `collect` builds it. Results go to the terminal.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use pm_config::Workspace;
use pm_llm::Axes;
use serde::Deserialize;

use crate::cache::Cache;
use crate::collect::{CollectOptions, plan, pool};
use crate::velocity::{
    Classifiers, Prepared, VelocitySettings, answer_key, cached_answer, prepare_package, read_packages, repos,
};

/// Every axis must agree at least this well (quadratic-weighted κ ≥ 0.6: "substantial").
pub const MIN_KAPPA: f64 = 0.6;
/// Total points within ±10% of the reference (plan §6.5).
pub const MAX_DRIFT_PCT: f64 = 10.0;
/// Two runs give the same points for at least this share of packages.
pub const MIN_RETEST_PCT: f64 = 80.0;
/// Fewer matched packages than this give no verdict.
pub const MIN_PACKAGES: usize = 20;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Reference {
    pub project: String,
    pub id: String,
    #[serde(rename = "R1")]
    pub r1: u8,
    #[serde(rename = "R2")]
    pub r2: u8,
    #[serde(rename = "R3")]
    pub r3: u8,
    #[serde(rename = "R4")]
    pub r4: u8,
    #[serde(rename = "R5")]
    pub r5: u8,
    #[serde(rename = "R6")]
    pub r6: u8,
}

impl Reference {
    pub fn axes(&self) -> Axes {
        Axes([self.r1, self.r2, self.r3, self.r4, self.r5, self.r6])
    }
}

/// A JSON array of `{project, id, R1..R6}`, or a CSV with that header.
pub fn read_reference(path: &Path) -> Result<Vec<Reference>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rows: Vec<Reference> = if text.trim_start().starts_with('[') {
        serde_json::from_str(&text).with_context(|| format!("{}", path.display()))?
    } else {
        let mut lines = text.lines().filter(|l| !l.trim().is_empty());
        let header: Vec<&str> = lines.next().unwrap_or("").split(',').map(str::trim).collect();
        let col = |name: &str| {
            header
                .iter()
                .position(|h| *h == name)
                .ok_or_else(|| anyhow!("{}: no `{name}` column", path.display()))
        };
        let cols = [
            col("project")?,
            col("id")?,
            col("R1")?,
            col("R2")?,
            col("R3")?,
            col("R4")?,
            col("R5")?,
            col("R6")?,
        ];
        lines
            .enumerate()
            .map(|(i, l)| {
                let f: Vec<&str> = l.split(',').map(str::trim).collect();
                let get = |c: usize| {
                    f.get(c)
                        .copied()
                        .ok_or_else(|| anyhow!("line {}: too few fields", i + 2))
                };
                let n = |c: usize| -> Result<u8> { get(c)?.parse().map_err(|e| anyhow!("line {}: {e}", i + 2)) };
                Ok(Reference {
                    project: get(cols[0])?.to_string(),
                    id: get(cols[1])?.to_string(),
                    r1: n(cols[2])?,
                    r2: n(cols[3])?,
                    r3: n(cols[4])?,
                    r4: n(cols[5])?,
                    r5: n(cols[6])?,
                    r6: n(cols[7])?,
                })
            })
            .collect::<Result<_>>()?
    };
    if let Some(r) = rows.iter().find(|r| r.axes().0.iter().any(|a| *a > 3)) {
        bail!("{}: `{}` has an axis above 3", path.display(), r.id);
    }
    Ok(rows)
}

/// Quadratic-weighted Cohen's κ for levels 0..=3; `None` when it is undefined (no spread).
pub fn quadratic_kappa(a: &[u8], b: &[u8]) -> Option<f64> {
    const K: usize = 4;
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let n = a.len() as f64;
    let mut observed = [[0.0; K]; K];
    let (mut ha, mut hb) = ([0.0; K], [0.0; K]);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = ((*x as usize).min(K - 1), (*y as usize).min(K - 1));
        observed[x][y] += 1.0;
        ha[x] += 1.0;
        hb[y] += 1.0;
    }
    let (mut num, mut den) = (0.0, 0.0);
    for i in 0..K {
        for j in 0..K {
            let w = ((i as f64 - j as f64) / (K as f64 - 1.0)).powi(2);
            num += w * observed[i][j];
            den += w * ha[i] * hb[j] / n;
        }
    }
    (den > 0.0).then(|| 1.0 - num / den)
}

pub struct EvalOptions {
    pub classifier: String,
    pub as_of: chrono::NaiveDate,
    /// 2 = also score every package a second time, without the cache (test–retest).
    pub runs: u32,
    pub use_cache: bool,
}

/// One package scored by the candidate.
#[derive(Clone, Debug)]
pub struct Scored {
    pub id: String,
    pub reference: Axes,
    pub candidate: Axes,
    pub retest: Option<Axes>,
    pub low_confidence: bool,
    pub escalated: bool,
}

#[derive(Clone, Debug, Default)]
pub struct EvalReport {
    pub classifier: String,
    pub project: String,
    /// Reference rows for this project.
    pub reference: usize,
    /// Reference ids with no package in the repos (grouping or window differs).
    pub missing: Vec<String>,
    /// Packages the data policy kept from the candidate.
    pub refused: usize,
    /// Calls that failed.
    pub failed: Vec<(String, String)>,
    pub scored: Vec<Scored>,
}

impl EvalReport {
    pub fn kappa(&self, axis: usize) -> Option<f64> {
        let r: Vec<u8> = self.scored.iter().map(|s| s.reference.0[axis]).collect();
        let c: Vec<u8> = self.scored.iter().map(|s| s.candidate.0[axis]).collect();
        quadratic_kappa(&r, &c)
    }

    pub fn points(&self) -> (u32, u32) {
        (
            self.scored.iter().map(|s| s.reference.points()).sum(),
            self.scored.iter().map(|s| s.candidate.points()).sum(),
        )
    }

    /// Candidate total points vs the reference, in percent.
    pub fn drift_pct(&self) -> Option<f64> {
        let (r, c) = self.points();
        (r > 0).then(|| 100.0 * (c as f64 - r as f64) / r as f64)
    }

    pub fn points_exact_pct(&self) -> Option<f64> {
        pct(
            self.scored
                .iter()
                .filter(|s| s.reference.points() == s.candidate.points())
                .count(),
            self.scored.len(),
        )
    }

    pub fn retest_pct(&self) -> Option<f64> {
        let both: Vec<&Scored> = self.scored.iter().filter(|s| s.retest.is_some()).collect();
        pct(
            both.iter()
                .filter(|s| s.retest.map(|r| r.points()) == Some(s.candidate.points()))
                .count(),
            both.len(),
        )
    }

    pub fn low_confidence_pct(&self) -> Option<f64> {
        pct(
            self.scored.iter().filter(|s| s.low_confidence).count(),
            self.scored.len(),
        )
    }

    /// `Ok(())` = passes; else the reasons it does not.
    pub fn verdict(&self) -> std::result::Result<(), Vec<String>> {
        let mut why = Vec::new();
        if self.scored.len() < MIN_PACKAGES {
            why.push(format!(
                "only {} packages scored (need {MIN_PACKAGES})",
                self.scored.len()
            ));
        }
        for (i, name) in ["R1", "R2", "R3", "R4", "R5", "R6"].iter().enumerate() {
            match self.kappa(i) {
                Some(k) if k >= MIN_KAPPA => {}
                Some(k) => why.push(format!("{name} κ {k:.2} < {MIN_KAPPA}")),
                None => why.push(format!("{name} κ undefined (no spread in the scores)")),
            }
        }
        match self.drift_pct() {
            Some(d) if d.abs() <= MAX_DRIFT_PCT => {}
            Some(d) => why.push(format!("Velocity drift {d:+.1}% (limit ±{MAX_DRIFT_PCT}%)")),
            None => why.push("no reference points".into()),
        }
        if let Some(r) = self.retest_pct() {
            if r < MIN_RETEST_PCT {
                why.push(format!("test–retest {r:.0}% < {MIN_RETEST_PCT}%"));
            }
        }
        if why.is_empty() { Ok(()) } else { Err(why) }
    }
}

fn pct(n: usize, of: usize) -> Option<f64> {
    (of > 0).then(|| 100.0 * n as f64 / of as f64)
}

/// This project's packages named by `ids`, prepared for `classifier` exactly as `collect` prepares
/// them.
struct Matched<'w> {
    set: pm_llm::QuestionSet,
    classifiers: Classifiers<'w>,
    /// Matched id (the one asked for), the package as prepared, local-only.
    todo: Vec<(String, Prepared, bool)>,
    /// Ids with no package in the repos (grouping or window differs).
    missing: Vec<String>,
    /// Packages the data policy kept from the classifier.
    refused: usize,
}

fn matched<'w>(
    ws: &'w Workspace,
    ids: &[&str],
    classifier: &str,
    as_of: chrono::NaiveDate,
    use_cache: bool,
) -> Result<Matched<'w>> {
    let config = &ws.config;
    if !config.classifiers.contains_key(classifier) {
        bail!("no [classifiers.{classifier}] in pmx.toml");
    }
    let wanted: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
    if wanted.is_empty() {
        bail!("no packages listed for project `{}`", config.project.name);
    }
    let mut copts = CollectOptions::new(as_of);
    copts.fetch = false;
    let plans = plan(ws, &copts)?;
    let mut vrepos = repos(ws, &plans);
    for r in &mut vrepos {
        r.classifier = Some(classifier.to_string());
    }
    let cls = config.classifier(pm_classify::TestRules::Spec);
    let identity = pm_config::Identity::new(&config.people);
    let settings = VelocitySettings {
        ws,
        as_of,
        classifier: &cls,
        identity: &identity,
        use_cache,
        progress: pm_progress::Progress::off(),
    };
    let stored = crate::pulls::stored(ws)?;
    let mut prs = crate::velocity::PrIndex::new(ws, &stored.repos, stored.missing);
    prs.add_merges(&vrepos, config.project.range_start)?;
    let (packages, gits) = read_packages(&settings, &vrepos, &prs, config.project.range_start)?;
    let set = crate::velocity::rubric(ws)?;
    let classifiers = Classifiers::build(config, [classifier], set.template);
    let provider = classifiers.get(classifier).map_err(|e| anyhow!(e))?;

    // Exact ids first. An author-week id that names a person by a raw git name (the prototype's
    // ids; `[people]` may call them differently) matches through the package's aliases, unless
    // several listed ids point at one package: `[people]` merged what the list scored apart, so
    // neither compares like for like.
    let mut matched: Vec<(&str, &crate::velocity::WorkPackage)> = Vec::new();
    let mut found = std::collections::BTreeSet::new();
    let mut by_alias: BTreeMap<String, Vec<&crate::velocity::WorkPackage>> = BTreeMap::new();
    for wp in &packages {
        match wanted.get(wp.id.as_str()) {
            Some(id) => {
                found.insert(*id);
                matched.push((id, wp));
            }
            None => {
                for a in wp.aliases() {
                    by_alias.entry(a).or_default().push(wp);
                }
            }
        }
    }
    let mut alias_ids: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for id in &wanted {
        if found.contains(id) {
            continue;
        }
        if let Some([wp]) = by_alias.get(*id).map(Vec::as_slice) {
            alias_ids.entry(wp.id.as_str()).or_default().push(id);
        }
    }
    for wp in &packages {
        if let Some([id]) = alias_ids.get(wp.id.as_str()).map(Vec::as_slice) {
            found.insert(*id);
            matched.push((id, wp));
        }
    }

    let mut todo = Vec::new();
    let mut refused = 0;
    for (id, wp) in matched {
        let (prepared, _, local_only) = prepare_package(wp, &vrepos, &gits, &cls, &set)?;
        if local_only && !provider.local() {
            refused += 1;
            continue;
        }
        todo.push((id.to_string(), prepared, local_only));
    }
    let missing = wanted
        .iter()
        .filter(|id| !found.contains(**id))
        .map(|s| s.to_string())
        .collect();
    Ok(Matched {
        set,
        classifiers,
        todo,
        missing,
        refused,
    })
}

impl Matched<'_> {
    /// One answer per package, through the answer cache when `use_cache` (`collect` shares it).
    fn score(&self, ws: &Workspace, classifier: &str, use_cache: bool) -> Vec<Result<pm_llm::Gated>> {
        let config = &ws.config;
        let workers = config.classifiers[classifier].jobs.unwrap_or(4).clamp(1, 32);
        let cache_path = ws.state_dir().join("cache.sqlite");
        let set = &self.set;
        pool(
            &self.todo,
            workers,
            |(id, prepared, local_only)| -> Result<pm_llm::Gated> {
                let key = self
                    .classifiers
                    .key(classifier, *local_only)
                    .map(|k| answer_key(&config.project.name, id, &prepared.state, &set.id, &k))
                    .map_err(|e| anyhow!(e))?;
                let cache = if use_cache {
                    Some(Cache::open(&cache_path)?)
                } else {
                    None
                };
                let primary = self.classifiers.get(classifier).map_err(|e| anyhow!(e))?;
                if let Some(c) = &cache {
                    if let Some(g) = cached_answer(c, &key, &primary.identity())? {
                        return Ok(g);
                    }
                }
                let g = self.classifiers.ask(classifier, *local_only, set, prepared)?;
                if let Some(c) = &cache {
                    c.put_answer(&key, &g)?;
                }
                Ok(g)
            },
        )
    }
}

/// Score this workspace's share of the reference set with `opts.classifier`.
pub fn eval(ws: &Workspace, reference: &[Reference], opts: &EvalOptions) -> Result<EvalReport> {
    let config = &ws.config;
    let wanted: BTreeMap<&str, &Reference> = reference
        .iter()
        .filter(|r| r.project == config.project.name)
        .map(|r| (r.id.as_str(), r))
        .collect();
    if wanted.is_empty() {
        bail!(
            "the reference set has no packages for project `{}`",
            config.project.name
        );
    }
    let ids: Vec<&str> = wanted.keys().copied().collect();
    let m = matched(ws, &ids, &opts.classifier, opts.as_of, opts.use_cache)?;
    let set = &m.set;
    let mut report = EvalReport {
        classifier: m.classifiers.get(&opts.classifier).map_err(|e| anyhow!(e))?.identity(),
        project: config.project.name.clone(),
        reference: wanted.len(),
        missing: m.missing.clone(),
        refused: m.refused,
        ..Default::default()
    };
    let first = m.score(ws, &opts.classifier, opts.use_cache);
    let workers = config.classifiers[&opts.classifier].jobs.unwrap_or(4).clamp(1, 32);
    let second: Vec<Option<Result<pm_llm::Gated>>> = if opts.runs >= 2 {
        pool(&m.todo, workers, |(_, prepared, local_only)| {
            m.classifiers.ask(&opts.classifier, *local_only, set, prepared)
        })
        .into_iter()
        .map(Some)
        .collect()
    } else {
        (0..m.todo.len()).map(|_| None).collect()
    };
    for (((id, prepared, _), a), b) in m.todo.iter().zip(first).zip(second) {
        let r = wanted[id.as_str()];
        match a {
            Ok(g) => {
                let candidate = set.axes(&g.response.answers, &prepared.facts)?;
                let retest = match b {
                    Some(Ok(g2)) => Some(set.axes(&g2.response.answers, &prepared.facts)?),
                    Some(Err(e)) => {
                        report.failed.push((r.id.clone(), format!("retest: {e:#}")));
                        None
                    }
                    None => None,
                };
                report.scored.push(Scored {
                    id: r.id.clone(),
                    reference: r.axes(),
                    candidate,
                    retest,
                    low_confidence: g.low_confidence,
                    escalated: g.escalated,
                });
            }
            Err(e) => report.failed.push((r.id.clone(), format!("{e:#}"))),
        }
    }
    Ok(report)
}

/// Package ids to score: a CSV with an `id` column (rows of other projects skipped when it has a
/// `project` column), or one id per line.
pub fn read_ids(path: &Path, project: &str) -> Result<Vec<String>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty());
    let first = lines.next().unwrap_or("");
    let header: Vec<&str> = first.split(',').map(str::trim).collect();
    let Some(id_col) = header.iter().position(|h| *h == "id") else {
        return Ok(std::iter::once(first).chain(lines).map(String::from).collect());
    };
    let project_col = header.iter().position(|h| *h == "project");
    Ok(lines
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            if project_col.is_some_and(|c| f.get(c) != Some(&project)) {
                return None;
            }
            f.get(id_col).map(|s| s.to_string())
        })
        .collect())
}

/// What `pmx classifier score` did.
#[derive(Debug, Default)]
pub struct ScoreReport {
    /// `project,id,R1..R6`: a reference set for `classifier eval`.
    pub csv: String,
    pub scored: usize,
    pub missing: Vec<String>,
    pub refused: usize,
    pub failed: Vec<(String, String)>,
}

/// Score the listed packages of this project with `classifier`, one package per call, and write
/// them as a reference set (real package ids, never pseudonyms).
pub fn score(
    ws: &Workspace,
    ids: &[String],
    classifier: &str,
    as_of: chrono::NaiveDate,
    use_cache: bool,
) -> Result<ScoreReport> {
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let m = matched(ws, &ids, classifier, as_of, use_cache)?;
    let mut out = ScoreReport {
        csv: "project,id,R1,R2,R3,R4,R5,R6\n".into(),
        missing: m.missing.clone(),
        refused: m.refused,
        ..Default::default()
    };
    let project = &ws.config.project.name;
    if project.contains(',') {
        bail!("project name `{project}` has a comma: cannot write CSV");
    }
    for ((id, prepared, _), g) in m.todo.iter().zip(m.score(ws, classifier, use_cache)) {
        match g.and_then(|g| Ok(m.set.axes(&g.response.answers, &prepared.facts)?)) {
            Ok(a) => {
                let a = a.0;
                out.csv.push_str(&format!(
                    "{project},{id},{},{},{},{},{},{}\n",
                    a[0], a[1], a[2], a[3], a[4], a[5]
                ));
                out.scored += 1;
            }
            Err(e) => out.failed.push((id.clone(), format!("{e:#}"))),
        }
    }
    Ok(out)
}

/// The report as text.
pub fn render(r: &EvalReport) -> String {
    let mut s = String::new();
    let opt = |x: Option<f64>, d: usize| x.map_or("–".to_string(), |v| format!("{v:.d$}"));
    s.push_str(&format!("classifier eval · {} · {}\n", r.project, r.classifier));
    s.push_str(&format!(
        "packages: {} scored of {} in the reference ({} not found in the repos, {} refused by the data policy, {} failed)\n",
        r.scored.len(),
        r.reference,
        r.missing.len(),
        r.refused,
        r.failed.len()
    ));
    s.push_str("agreement (quadratic-weighted κ):");
    for (i, name) in ["R1", "R2", "R3", "R4", "R5", "R6"].iter().enumerate() {
        s.push_str(&format!(" {name} {}", opt(r.kappa(i), 2)));
    }
    let (rp, cp) = r.points();
    s.push_str(&format!(
        "\npoints: {cp} vs {rp} reference (drift {}%), same points on {}% of packages\n",
        opt(r.drift_pct(), 1),
        opt(r.points_exact_pct(), 0)
    ));
    if r.scored.iter().any(|x| x.retest.is_some()) {
        s.push_str(&format!(
            "test–retest: same points on {}% of packages\n",
            opt(r.retest_pct(), 0)
        ));
    }
    s.push_str(&format!(
        "unsure answers: {}% · escalated to the fallback: {}\n",
        opt(r.low_confidence_pct(), 0),
        r.scored.iter().filter(|x| x.escalated).count()
    ));
    if let Some((id, e)) = r.failed.first() {
        s.push_str(&format!("first failure ({id}): {e}\n"));
    }
    match r.verdict() {
        Ok(()) => s.push_str("PASS\n"),
        Err(why) => s.push_str(&format!("FAIL: {}\n", why.join("; "))),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kappa() {
        assert_eq!(quadratic_kappa(&[0, 1, 2, 3], &[0, 1, 2, 3]), Some(1.0));
        assert_eq!(quadratic_kappa(&[1, 1, 1], &[1, 1, 1]), None, "no spread");
        // Off by one everywhere is still fairly good; reversed is negative.
        let k = quadratic_kappa(&[0, 1, 2, 3, 0, 1, 2, 3], &[1, 2, 3, 3, 0, 1, 2, 2]).unwrap();
        assert!(k > 0.6 && k < 1.0, "{k}");
        assert!(quadratic_kappa(&[0, 1, 2, 3], &[3, 2, 1, 0]).unwrap() < 0.0);
        // By hand: observed weight 1/9 over 4, expected (2/9 + 4 + 2 + 8/9) / 4 → 1 − (1/9)/(64/36).
        let k = quadratic_kappa(&[0, 0, 3, 3], &[0, 1, 3, 3]).unwrap();
        assert!((k - 0.9375).abs() < 1e-12, "{k}");
    }

    #[test]
    fn reads_csv_and_json() {
        let dir = tempfile::tempdir().unwrap();
        let csv = dir.path().join("ref.csv");
        std::fs::write(
            &csv,
            "project,id,R1,R2,R3,R4,R5,R6\nAcme,SHOP-1,1,0,1,2,1,2\nAcme,p~2026-W02,0,0,0,0,0,1\n",
        )
        .unwrap();
        let r = read_reference(&csv).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].axes(), Axes([1, 0, 1, 2, 1, 2]));
        let json = dir.path().join("ref.json");
        std::fs::write(
            &json,
            r#"[{"project":"Acme","id":"X-1","R1":3,"R2":0,"R3":0,"R4":0,"R5":0,"R6":0}]"#,
        )
        .unwrap();
        assert_eq!(read_reference(&json).unwrap()[0].r1, 3);
        std::fs::write(&csv, "project,id,R1,R2,R3,R4,R5,R6\nAcme,X,4,0,0,0,0,0\n").unwrap();
        assert!(read_reference(&csv).unwrap_err().to_string().contains("above 3"));
    }

    #[test]
    fn verdict() {
        let s = |r: [u8; 6], c: [u8; 6]| Scored {
            id: "x".into(),
            reference: Axes(r),
            candidate: Axes(c),
            retest: Some(Axes(c)),
            low_confidence: false,
            escalated: false,
        };
        let mut rep = EvalReport::default();
        for i in 0..24u8 {
            let a = [i % 4, (i / 2) % 4, (i / 3) % 4, (i / 4) % 4, (i + 1) % 4, (i + 2) % 4];
            rep.scored.push(s(a, a));
        }
        assert_eq!(rep.verdict(), Ok(()));
        assert_eq!(rep.drift_pct(), Some(0.0));
        rep.scored.truncate(5);
        assert!(rep.verdict().unwrap_err()[0].contains("only 5 packages"));
    }
}
