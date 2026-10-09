//! The stored truth: additive components per day (spec §1.1). Weeks, months and any custom range
//! are exact roll-ups of these.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use pm_classify::Role;
use serde::{Deserialize, Serialize};

fn is_zero<T: Default + PartialEq>(x: &T) -> bool {
    *x == T::default()
}

/// Commit components for one group of commits (human or AI-assisted, spec §6).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Group {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub commits: u32,
    /// Σ added lines in source files.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub added: u64,
    /// Per commit Σ(added + deleted) over source files; commits with 0 are not listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sizes: Vec<u64>,
    /// Commits touching ≥ 1 prod file (the tests/docs-with-code denominator).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub prod_commits: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub tests_with_prod: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub docs_with_prod: u32,
}

impl Group {
    pub fn is_empty(&self) -> bool {
        *self == Group::default()
    }

    pub fn merge(&mut self, o: &Group) {
        self.commits += o.commits;
        self.added += o.added;
        self.sizes.extend_from_slice(&o.sizes);
        self.prod_commits += o.prod_commits;
        self.tests_with_prod += o.tests_with_prod;
        self.docs_with_prod += o.docs_with_prod;
    }
}

/// person → role → technology → added source lines.
pub type Lines = BTreeMap<String, BTreeMap<Role, BTreeMap<String, u64>>>;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Day {
    /// Commits without an AI trailer, by included authors (spec §1.3).
    #[serde(default, skip_serializing_if = "Group::is_empty")]
    pub human: Group,
    /// AI-assisted commits (spec §6).
    #[serde(default, skip_serializing_if = "Group::is_empty")]
    pub ai: Group,
    /// Rework walk (spec §3.1): non-trivial source lines added, and lines reworked. Not
    /// author-filtered.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rework_added: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reworked: u64,
    /// person → commits.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub person_commits: BTreeMap<String, u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lines: Lines,
}

pub type Days = BTreeMap<NaiveDate, Day>;

impl Day {
    pub fn is_empty(&self) -> bool {
        *self == Day::default()
    }

    pub fn commits(&self) -> u32 {
        self.human.commits + self.ai.commits
    }

    pub fn added(&self) -> u64 {
        self.human.added + self.ai.added
    }

    pub fn merge(&mut self, o: &Day) {
        self.human.merge(&o.human);
        self.ai.merge(&o.ai);
        self.rework_added += o.rework_added;
        self.reworked += o.reworked;
        for (p, n) in &o.person_commits {
            *self.person_commits.entry(p.clone()).or_default() += n;
        }
        for (p, roles) in &o.lines {
            let mine = self.lines.entry(p.clone()).or_default();
            for (r, techs) in roles {
                let mr = mine.entry(*r).or_default();
                for (t, n) in techs {
                    *mr.entry(t.clone()).or_default() += n;
                }
            }
        }
    }

    pub fn add_lines(&mut self, person: &str, role: Role, tech: &str, n: u64) {
        *self
            .lines
            .entry(person.to_string())
            .or_default()
            .entry(role)
            .or_default()
            .entry(tech.to_string())
            .or_default() += n;
    }

    /// The same day with every person renamed through `f`.
    pub fn rename_people(&self, f: impl Fn(&str) -> String) -> Day {
        Day {
            person_commits: self.person_commits.iter().map(|(p, n)| (f(p), *n)).collect(),
            lines: self.lines.iter().map(|(p, r)| (f(p), r.clone())).collect(),
            ..self.clone()
        }
    }
}

/// Sum the days in `[start, end]` (inclusive).
pub fn aggregate(days: &Days, start: NaiveDate, end: NaiveDate) -> Day {
    let mut agg = Day::default();
    if start > end {
        return agg;
    }
    for d in days.range(start..=end).map(|(_, d)| d) {
        agg.merge(d);
    }
    agg
}

/// Merge `other` into `into`, day by day.
pub fn merge_days(into: &mut Days, other: &Days) {
    for (date, d) in other {
        into.entry(*date).or_default().merge(d);
    }
}
