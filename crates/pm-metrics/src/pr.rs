//! Pull-request flow (spec §8): per merged PR, the components each day stores, and mentoring pairs
//! under the unified expert rule.

use std::collections::HashMap;

use chrono::{Days as ChronoDays, NaiveDate};
use pm_classify::Role;
use serde::{Deserialize, Serialize};

use crate::day::Days;

/// Expert status counts the PRs merged in this many days before a PR's merge date.
pub const EXPERT_WINDOW_DAYS: u64 = 365;
/// An expert in a stack has at least this many merged PRs in it…
pub const EXPERT_MIN_PRS: u32 = 3;
/// …and at least this share of the stack's top author's count.
pub const EXPERT_SHARE: f64 = 0.25;

/// One merged PR, host-independent and resolved to people.
#[derive(Clone, Debug, PartialEq)]
pub struct PrInput {
    /// Merge date in the host's recorded offset (spec §1.1).
    pub merged: NaiveDate,
    /// `created → merged`, in milliseconds.
    pub cycle_ms: u64,
    /// `created → first review` by a non-author human, in milliseconds.
    pub first_review_ms: Option<u64>,
    /// Canonical person; `None` for a deleted account.
    pub author: Option<String>,
    /// People with a review on it (non-author, non-bot).
    pub reviewers: Vec<String>,
    /// The repo's role, or the dominant role by added lines in a per-file repo.
    pub stack: Option<Role>,
    /// `additions + deletions` (GitHub only).
    pub size: Option<u64>,
}

fn is_zero<T: Default + PartialEq>(x: &T) -> bool {
    *x == T::default()
}

/// PR components of one day, by merge date.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrDay {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub merged: u32,
    /// Per PR, `created → merged` in milliseconds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cycle_ms: Vec<u64>,
    /// PRs with a review by a non-author human.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub reviewed: u32,
    /// Per reviewed PR, `created → first review` in milliseconds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wait_ms: Vec<u64>,
    /// Per PR with line counts, `additions + deletions`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sizes: Vec<u64>,
    /// Reviewed PRs by a non-expert with an expert reviewer.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub mentored: u32,
}

impl PrDay {
    pub fn is_empty(&self) -> bool {
        *self == PrDay::default()
    }

    pub fn merge(&mut self, o: &PrDay) {
        self.merged += o.merged;
        self.cycle_ms.extend_from_slice(&o.cycle_ms);
        self.reviewed += o.reviewed;
        self.wait_ms.extend_from_slice(&o.wait_ms);
        self.sizes.extend_from_slice(&o.sizes);
        self.mentored += o.mentored;
    }
}

/// Merged-PR counts per (stack, person) over a sliding window.
#[derive(Default)]
struct Expertise {
    counts: HashMap<(Role, String), u32>,
}

impl Expertise {
    fn add(&mut self, p: &PrInput, delta: i32) {
        if let (Some(s), Some(a)) = (p.stack, &p.author) {
            let c = self.counts.entry((s, a.clone())).or_default();
            *c = c.saturating_add_signed(delta);
        }
    }

    /// Spec §8: ≥ 3 merged PRs in the stack and ≥ 25% of the stack's top author's count.
    fn is_expert(&self, person: &str, stack: Role) -> bool {
        let c = self.counts.get(&(stack, person.to_string())).copied().unwrap_or(0);
        let top = self
            .counts
            .iter()
            .filter(|((s, _), _)| *s == stack)
            .map(|(_, n)| *n)
            .max()
            .unwrap_or(0);
        c >= EXPERT_MIN_PRS && c as f64 >= EXPERT_SHARE * top as f64
    }
}

/// Day components of the PRs. Expert status for a PR counts the PRs merged in the
/// [`EXPERT_WINDOW_DAYS`] days before its merge date, so a PR's mentoring flag never changes when
/// later PRs arrive.
pub fn pr_days(prs: &[PrInput]) -> Days {
    let mut order: Vec<&PrInput> = prs.iter().collect();
    order.sort_by_key(|p| p.merged);
    let mut days = Days::new();
    let mut window = Expertise::default();
    let (mut lo, mut hi) = (0, 0);
    for p in &order {
        while hi < order.len() && order[hi].merged < p.merged {
            window.add(order[hi], 1);
            hi += 1;
        }
        let from = p.merged - ChronoDays::new(EXPERT_WINDOW_DAYS);
        while lo < hi && order[lo].merged < from {
            window.add(order[lo], -1);
            lo += 1;
        }
        let d = &mut days.entry(p.merged).or_default().pr;
        d.merged += 1;
        d.cycle_ms.push(p.cycle_ms);
        if let Some(s) = p.size {
            d.sizes.push(s);
        }
        if let Some(w) = p.first_review_ms {
            d.reviewed += 1;
            d.wait_ms.push(w);
            if let (Some(a), Some(s)) = (&p.author, p.stack) {
                if !window.is_expert(a, s) && p.reviewers.iter().any(|r| window.is_expert(r, s)) {
                    d.mentored += 1;
                }
            }
        }
    }
    days
}

/// Hours from milliseconds, in the prototype's order of operations (seconds first).
pub fn hours(ms: u64) -> f64 {
    ms as f64 / 1000.0 / 3600.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn pr(merged: &str, author: &str, reviewers: &[&str]) -> PrInput {
        PrInput {
            merged: d(merged),
            cycle_ms: 3_600_000,
            first_review_ms: (!reviewers.is_empty()).then_some(1_800_000),
            author: Some(author.into()),
            reviewers: reviewers.iter().map(|r| r.to_string()).collect(),
            stack: Some(Role::Backend),
            size: None,
        }
    }

    #[test]
    fn counts_and_waits() {
        let days = pr_days(&[pr("2025-03-01", "ann", &["bob"]), pr("2025-03-01", "bob", &[])]);
        let day = &days[&d("2025-03-01")].pr;
        assert_eq!((day.merged, day.reviewed), (2, 1));
        assert_eq!(day.wait_ms, vec![1_800_000]);
        assert!(day.sizes.is_empty());
    }

    #[test]
    fn mentoring_uses_only_earlier_prs() {
        // bob has 3 backend PRs before March; cy has 1. ann has none.
        let mut prs = vec![
            pr("2025-01-10", "bob", &[]),
            pr("2025-01-11", "bob", &[]),
            pr("2025-01-12", "bob", &[]),
            pr("2025-01-13", "cy", &[]),
            // A non-expert reviewed by an expert: a mentoring pair.
            pr("2025-03-01", "ann", &["bob"]),
            // Reviewed by a non-expert only: not one.
            pr("2025-03-02", "ann", &["cy"]),
            // The expert's own PR, reviewed by a non-expert: not one.
            pr("2025-03-03", "bob", &["ann"]),
        ];
        let days = pr_days(&prs);
        let m = |s: &str| days[&d(s)].pr.mentored;
        assert_eq!((m("2025-03-01"), m("2025-03-02"), m("2025-03-03")), (1, 0, 0));

        // A year later bob's PRs have left the window: no longer an expert.
        prs.push(pr("2026-01-20", "ann", &["bob"]));
        assert_eq!(pr_days(&prs)[&d("2026-01-20")].pr.mentored, 0);
    }

    #[test]
    fn expert_needs_a_quarter_of_the_top_count() {
        let mut prs: Vec<PrInput> = (1..=20).map(|i| pr(&format!("2025-01-{i:02}"), "bob", &[])).collect();
        prs.extend((21..=24).map(|i| pr(&format!("2025-01-{i:02}"), "cy", &[])));
        // cy: 4 PRs < 25% of 20 → not an expert, so bob reviewing cy is a pair, cy reviewing dee is not.
        prs.push(pr("2025-02-01", "cy", &["bob"]));
        prs.push(pr("2025-02-01", "dee", &["cy"]));
        assert_eq!(pr_days(&prs)[&d("2025-02-01")].pr.mentored, 1);
        // The next day cy's fifth PR counts: 5 ≥ 25% of 20, an expert now.
        prs.push(pr("2025-02-02", "dee", &["cy"]));
        assert_eq!(pr_days(&prs)[&d("2025-02-02")].pr.mentored, 1);
    }

    #[test]
    fn hours_like_the_prototype() {
        assert_eq!(hours(5_400_000), 1.5);
        assert_eq!(hours(1_234), 1.234 / 3600.0);
    }
}
