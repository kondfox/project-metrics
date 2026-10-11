//! Velocity (spec §4): scored work packages → per-day components, and points per FTE-day.

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::day::Days;

fn is_zero<T: Default + PartialEq>(x: &T) -> bool {
    *x == T::default()
}

/// One work package as the project file lists it (spec §10.3 output contract, plus what the
/// roll-up needs).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Package {
    /// Ticket id, or `<person>~<ISO week>` (a pseudonym in `project.json`).
    pub id: String,
    /// The last commit's author date: the package counts in that week and month.
    pub date: NaiveDate,
    pub repos: Vec<String>,
    pub commits: u32,
    /// Added lines in source files.
    pub added: u64,
    /// Ticket id plus the first commit subject.
    pub description: String,
    /// R1–R6; `None` when it could not be scored.
    pub axes: Option<[u8; 6]>,
    pub points: Option<u32>,
    pub size: Option<String>,
    /// The package touched a test file.
    pub tests: bool,
    /// The diff sent was cut at the size cap.
    pub truncated: bool,
    /// The classifier (`provider:model:template`) and the model that answered.
    pub classifier: Option<String>,
    pub model: Option<String>,
    /// The person of an author-week package; replaced by a pseudonym before writing.
    #[serde(skip)]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub author: Option<String>,
}

impl Package {
    /// Points of work that needs a unit test or more (R6 ≥ 2) but touched no test (spec §4).
    pub fn verification_debt(&self) -> u32 {
        match (self.axes, self.points) {
            (Some(a), Some(p)) if a[5] >= 2 && !self.tests => p,
            _ => 0,
        }
    }
}

/// Velocity components of one day: the packages dated that day.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VelocityDay {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub packages: u32,
    /// Packages that could not be scored: a range holding any has no Velocity (a partial sum would
    /// read as a drop).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unscored: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub points: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub commits: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub added: u64,
    /// Verification-debt points.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub debt: u32,
}

impl VelocityDay {
    pub fn is_empty(&self) -> bool {
        *self == VelocityDay::default()
    }

    pub fn merge(&mut self, o: &VelocityDay) {
        self.packages += o.packages;
        self.unscored += o.unscored;
        self.points += o.points;
        self.commits += o.commits;
        self.added += o.added;
        self.debt += o.debt;
    }
}

pub fn velocity_days(packages: &[Package]) -> Days {
    let mut days = Days::new();
    for p in packages {
        let v = &mut days.entry(p.date).or_default().velocity;
        v.packages += 1;
        v.commits += p.commits;
        v.added += p.added;
        match p.points {
            Some(pts) => {
                v.points += pts;
                v.debt += p.verification_debt();
            }
            None => v.unscored += 1,
        }
    }
    days
}

/// FTE from `from` to `to` (inclusive; open = until further notice), spec §10.2.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FtePeriod {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub fte: f64,
}

/// What a range's Velocity needs besides its components.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VelocityOptions {
    /// Packages are scored from this date: a range starting earlier has no Velocity.
    pub since: NaiveDate,
    /// Business days are counted up to here (a partial month counts its days so far).
    pub as_of: NaiveDate,
    /// Empty: FTE unknown, so the denominator is active engineer-days (marked approximate).
    pub fte: Vec<FtePeriod>,
}

impl VelocityOptions {
    /// FTE on `d`; the last matching period wins.
    pub fn fte_on(&self, d: NaiveDate) -> Option<f64> {
        self.fte
            .iter()
            .rev()
            .find(|p| p.from.is_none_or(|f| f <= d) && p.to.is_none_or(|t| d <= t))
            .map(|p| p.fte)
    }

    /// Σ FTE over the business days (Mon–Fri) of `[start, end]`, capped at `as_of`; `None` when a
    /// business day has no FTE.
    pub fn fte_days(&self, start: NaiveDate, end: NaiveDate) -> Option<f64> {
        if self.fte.is_empty() {
            return None;
        }
        let mut sum = 0.0;
        let mut d = start;
        while d <= end.min(self.as_of) {
            if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
                sum += self.fte_on(d)?;
            }
            d = d.succ_opt()?;
        }
        Some(sum)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn fte_days_over_business_days() {
        let o = VelocityOptions {
            since: d("2026-01-01"),
            as_of: d("2026-09-10"),
            fte: vec![
                FtePeriod {
                    from: None,
                    to: None,
                    fte: 2.0,
                },
                FtePeriod {
                    from: Some(d("2026-09-01")),
                    to: None,
                    fte: 4.0,
                },
            ],
        };
        // Aug 2026: 21 business days at 2 FTE.
        assert_eq!(o.fte_days(d("2026-08-01"), d("2026-08-31")), Some(42.0));
        // Sep: Sep 1–10 has 8 business days (capped at as_of), at 4 FTE.
        assert_eq!(o.fte_days(d("2026-09-01"), d("2026-09-30")), Some(32.0));
        let gap = VelocityOptions {
            fte: vec![FtePeriod {
                from: Some(d("2026-08-15")),
                to: None,
                fte: 1.0,
            }],
            ..o.clone()
        };
        assert_eq!(gap.fte_days(d("2026-08-01"), d("2026-08-31")), None);
        assert_eq!(gap.fte_days(d("2026-08-17"), d("2026-08-23")), Some(5.0));
    }

    #[test]
    fn days_from_packages() {
        let p = |date: &str, axes: Option<[u8; 6]>, pts: Option<u32>, tests: bool| Package {
            id: "X-1".into(),
            date: d(date),
            repos: vec!["api".into()],
            commits: 2,
            added: 100,
            description: String::new(),
            axes,
            points: pts,
            size: None,
            tests,
            truncated: false,
            classifier: None,
            model: None,
            author: None,
        };
        let days = velocity_days(&[
            p("2026-08-03", Some([2, 0, 1, 2, 1, 2]), Some(3), false),
            p("2026-08-03", Some([0, 0, 0, 1, 0, 2]), Some(1), true),
            p("2026-08-04", None, None, false),
        ]);
        let v = &days[&d("2026-08-03")].velocity;
        assert_eq!((v.packages, v.points, v.debt, v.commits, v.added), (2, 4, 3, 4, 200));
        assert_eq!(days[&d("2026-08-04")].velocity.unscored, 1);
    }
}
