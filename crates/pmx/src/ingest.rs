//! One repo's commits and rework counts → per-day components (spec §2, §3.1–3.3, §5, §6).

use std::collections::BTreeSet;

use chrono::NaiveDate;
use pm_classify::{Classifier, FileClass};
use pm_config::{Identity, RepoRole};
use pm_git::{Commit, ReworkCounts};
use pm_metrics::Days;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RepoIngest {
    /// Components per author date, from `range_start` on (people by canonical name).
    pub days: Days,
    /// Every commit date from `range_start` on, by any author (bots included).
    pub commit_dates: BTreeSet<NaiveDate>,
}

/// AI-assisted (spec §6): a `Co-Authored-By: Claude` trailer anywhere in the message.
pub fn is_ai_assisted(message: &str) -> bool {
    message.to_lowercase().contains("co-authored-by: claude")
}

pub struct IngestContext<'a> {
    pub role: RepoRole,
    pub classifier: &'a Classifier,
    pub identity: &'a Identity,
    pub range_start: NaiveDate,
}

pub fn ingest(commits: &[Commit], rework: &ReworkCounts, cx: &IngestContext) -> RepoIngest {
    let mut out = RepoIngest::default();
    let cls = cx.classifier;
    for c in commits.iter().filter(|c| c.author_date >= cx.range_start) {
        out.commit_dates.insert(c.author_date);
        let author = cx.identity.resolve(&c.author_email, &c.author_name);
        if author.excluded.is_some() {
            continue;
        }
        let day = out.days.entry(c.author_date).or_default();
        let (mut added, mut size) = (0u64, 0u64);
        let (mut prod, mut test, mut doc) = (false, false, false);
        for f in &c.files {
            let class = cls.classify(&f.path);
            prod |= class == FileClass::Prod;
            test |= cls.touches_test(&f.path);
            doc |= cls.touches_doc(&f.path);
            if !matches!(class, FileClass::Prod | FileClass::Test) {
                continue;
            }
            added += f.added;
            size += f.added + f.deleted;
            if f.added > 0 {
                let role = match cx.role {
                    RepoRole::Fixed(r) => r,
                    RepoRole::PerFile => cls.role_of_file(&f.path),
                };
                let tech = cls.technology(&f.path).expect("source files have a technology");
                day.add_lines(&author.person, role, tech, f.added);
            }
        }
        let g = if is_ai_assisted(&c.message) {
            &mut day.ai
        } else {
            &mut day.human
        };
        g.commits += 1;
        g.added += added;
        if size > 0 {
            g.sizes.push(size);
        }
        if prod {
            g.prod_commits += 1;
            g.tests_with_prod += test as u32;
            g.docs_with_prod += doc as u32;
        }
        *day.person_commits.entry(author.person).or_default() += 1;
    }
    for (date, (added, reworked)) in rework.range(cx.range_start..) {
        let day = out.days.entry(*date).or_default();
        day.rework_added += added;
        day.reworked += reworked;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ai_trailer() {
        assert!(is_ai_assisted(
            "Fix\n\nCo-Authored-By: Claude Opus <noreply@anthropic.com>"
        ));
        assert!(is_ai_assisted("co-authored-by: claude"));
        assert!(!is_ai_assisted("Co-Authored-By: Jane <jane@example.com>"));
    }
}
