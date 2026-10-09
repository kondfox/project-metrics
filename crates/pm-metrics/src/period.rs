//! Buckets: ISO weeks and calendar months (spec §1.1).

use chrono::{Datelike, Days, NaiveDate, Weekday};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// `2026-W41` or `2026-10`.
    pub label: String,
    pub start: NaiveDate,
    /// Inclusive.
    pub end: NaiveDate,
}

pub fn month_label(d: NaiveDate) -> String {
    d.format("%Y-%m").to_string()
}

pub fn week_label(d: NaiveDate) -> String {
    let w = d.iso_week();
    format!("{}-W{:02}", w.year(), w.week())
}

pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1 exists")
}

pub fn month_end(d: NaiveDate) -> NaiveDate {
    let next = if d.month() == 12 {
        NaiveDate::from_ymd_opt(d.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1)
    }
    .expect("valid month");
    next.pred_opt().expect("not the first day ever")
}

pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Days::new(d.weekday().num_days_from_monday() as u64)
}

/// Calendar months overlapping `[from, to]`.
pub fn months(from: NaiveDate, to: NaiveDate) -> Vec<Bucket> {
    let mut out = Vec::new();
    let mut s = month_start(from);
    while s <= to {
        let e = month_end(s);
        out.push(Bucket {
            label: month_label(s),
            start: s,
            end: e,
        });
        s = e.succ_opt().expect("valid date");
    }
    out
}

/// ISO weeks (Monday–Sunday) overlapping `[from, to]`.
pub fn weeks(from: NaiveDate, to: NaiveDate) -> Vec<Bucket> {
    let mut out = Vec::new();
    let mut s = week_start(from);
    while s <= to {
        let e = s + Days::new(6);
        out.push(Bucket {
            label: week_label(s),
            start: s,
            end: e,
        });
        s = e.succ_opt().expect("valid date");
    }
    out
}

/// Is the month containing `as_of` still running on that day?
pub fn partial_month(as_of: NaiveDate) -> bool {
    as_of < month_end(as_of)
}

pub fn partial_week(as_of: NaiveDate) -> bool {
    as_of.weekday() != Weekday::Sun
}

/// The last calendar month that is complete on `as_of` (the default headline window).
pub fn last_complete_month(as_of: NaiveDate) -> Bucket {
    let s = if partial_month(as_of) {
        month_start(month_start(as_of).pred_opt().expect("valid"))
    } else {
        month_start(as_of)
    };
    Bucket {
        label: month_label(s),
        start: s,
        end: month_end(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn month_buckets() {
        let m = months(d("2024-11-15"), d("2025-02-01"));
        let labels: Vec<_> = m.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["2024-11", "2024-12", "2025-01", "2025-02"]);
        assert_eq!(m[1].end, d("2024-12-31"));
        assert_eq!(month_end(d("2024-02-10")), d("2024-02-29"));
    }

    #[test]
    fn iso_weeks() {
        // 2024-12-30 is a Monday in ISO week 2025-W01.
        let w = weeks(d("2024-12-31"), d("2025-01-06"));
        assert_eq!(w[0].label, "2025-W01");
        assert_eq!(w[0].start, d("2024-12-30"));
        assert_eq!(w[0].end, d("2025-01-05"));
        assert_eq!(w[1].label, "2025-W02");
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn partial_periods() {
        assert!(partial_month(d("2026-10-09")));
        assert!(!partial_month(d("2026-09-30")));
        assert!(partial_week(d("2026-10-09")));
        assert!(!partial_week(d("2026-10-11")));
        assert_eq!(last_complete_month(d("2026-10-09")).label, "2026-09");
        assert_eq!(last_complete_month(d("2026-09-30")).label, "2026-09");
    }
}
