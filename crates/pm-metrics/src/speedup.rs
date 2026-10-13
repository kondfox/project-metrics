//! Speed-up against phases and markers (spec §4.1): is the team faster now than when the current
//! phase began, and than before the AI tooling arrived? Medians of weekly values, a bootstrap
//! interval, and the Quality guardrails over the same windows.

use chrono::{Days as Span, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::day::{Days, aggregate};
use crate::metrics::{MetricOptions, metrics};
use crate::period::{Bucket, weeks};

/// A dated stretch of the project (`[[phase]]`), until the next phase starts.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub from: NaiveDate,
}

/// A dated event (`[[marker]]`); `kind = "ai"` marks AI tooling arriving.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub name: String,
    pub date: NaiveDate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

impl Marker {
    pub fn is_ai(&self) -> bool {
        self.kind.as_deref() == Some("ai")
    }
}

/// The first commit that added a `CLAUDE.md` or `AGENTS.md` (spec §4.1 item 3).
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adoption {
    pub repo: String,
    pub date: NaiveDate,
    /// The file(s) that commit added, e.g. `CLAUDE.md`, `app/CLAUDE.md`.
    pub files: Vec<String>,
    pub sha: String,
}

/// AI adoption as the repos show it: the project's first, and each repo's.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiAdoption {
    pub first: Option<Adoption>,
    /// Every repo with one, oldest first.
    pub repos: Vec<Adoption>,
}

/// What the speed-up comparisons are read against.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    /// Oldest first.
    #[serde(default)]
    pub phases: Vec<Phase>,
    /// Oldest first.
    #[serde(default)]
    pub markers: Vec<Marker>,
    #[serde(default)]
    pub ai_adoption: AiAdoption,
}

impl Timeline {
    /// The phase holding `d`.
    pub fn phase_at(&self, d: NaiveDate) -> Option<&Phase> {
        self.phases.iter().rev().find(|p| p.from <= d)
    }

    /// The AI markers to compare against, latest first: the configured ones, else the detected
    /// adoption (marked `detected`).
    pub fn ai_markers(&self) -> Vec<(Marker, Source)> {
        let configured: Vec<_> = self.markers.iter().filter(|m| m.is_ai()).rev().cloned().collect();
        if !configured.is_empty() {
            return configured.into_iter().map(|m| (m, Source::Configured)).collect();
        }
        self.ai_adoption
            .first
            .iter()
            .map(|a| {
                let marker = Marker {
                    name: format!(
                        "{} added",
                        a.files
                            .first()
                            .map_or("CLAUDE.md", |f| f.rsplit('/').next().unwrap_or(f))
                    ),
                    date: a.date,
                    kind: Some("ai".into()),
                };
                (marker, Source::Detected)
            })
            .collect()
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// From `pmx.toml`.
    Configured,
    /// From the repos (the first `CLAUDE.md` / `AGENTS.md`): to confirm in `pmx.toml`.
    Detected,
    /// No phases set: the weeks just before stand in.
    Fallback,
}

/// A run of complete weeks.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub weeks: u32,
}

/// Median now ÷ median before, with a 90% bootstrap interval.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ratio {
    pub now: f64,
    pub before: f64,
    pub ratio: f64,
    pub low: f64,
    pub high: f64,
    /// Weeks with a value on each side.
    pub weeks_now: u32,
    pub weeks_before: u32,
}

impl Ratio {
    /// The interval holds ×1.0: no clear change.
    pub fn unclear(&self) -> bool {
        self.low <= 1.0 && self.high >= 1.0
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Ok {
        before: Window,
        now: Window,
        /// Points per FTE-day; `None` when either side has no scored week or no output before.
        velocity: Option<Ratio>,
        /// Source lines added per FTE-day.
        volume: Option<Ratio>,
        /// Per active engineer-day: FTE is unknown.
        approx: bool,
        /// Rework % now − before, in points (pooled over each window).
        rework_pts: Option<f64>,
        /// Tests-with-code % now − before, in points.
        tests_pts: Option<f64>,
        /// The two windows lie in different phases.
        crosses_phase: bool,
        /// No work in the before-window (median 0 lines added a week): nothing to compare against, so
        /// there are no ratios and no guardrails.
        before_idle: bool,
        /// Other markers between the start of the before-window and the end of the now-window, e.g.
        /// a holiday lull or another AI step: they muddy the comparison.
        spans_markers: Vec<String>,
    },
    /// Not enough complete weeks yet.
    TooEarly { weeks: u32, since: NaiveDate },
}

/// One comparison, as the Velocity section shows it.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comparison {
    /// The phase or marker name.
    pub label: String,
    pub date: NaiveDate,
    pub source: Source,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// Both kinds of comparison for a range ending on `end`.
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpeedUps {
    pub end: NaiveDate,
    pub phase: Comparison,
    /// One per AI marker, latest first; empty when there is none, configured or detected.
    pub markers: Vec<Comparison>,
}

/// Fallback when no phases are set (spec §4.1 item 8).
const FALLBACK_WEEKS: u32 = 12;
const BOOTSTRAP_ROUNDS: usize = 1000;

/// The complete weeks within `[from, to]` (a week counts once it has ended by `to`).
fn complete_weeks(from: NaiveDate, to: NaiveDate) -> Vec<Bucket> {
    weeks(from, to)
        .into_iter()
        .filter(|w| w.start >= from && w.end <= to)
        .collect()
}

/// 8 weeks a side when both sides have them, else 4, else none.
fn window_size(before: usize, after: usize) -> Option<u32> {
    [8, 4]
        .into_iter()
        .find(|w| before >= *w as usize && after >= *w as usize)
}

fn median(xs: &[f64]) -> Option<f64> {
    if xs.is_empty() {
        return None;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    Some(if v.len() % 2 == 0 {
        (v[m - 1] + v[m]) / 2.0
    } else {
        v[m]
    })
}

/// A small deterministic generator: the same data always gives the same interval.
struct XorShift(u64);

impl XorShift {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Median ratio with a 90% interval from resampling each side's weeks.
fn ratio(before: &[f64], now: &[f64]) -> Option<Ratio> {
    let (b, n) = (median(before)?, median(now)?);
    if b <= 0.0 {
        return None;
    }
    let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
    let mut draws: Vec<f64> = (0..BOOTSTRAP_ROUNDS)
        .filter_map(|_| {
            let rb: Vec<f64> = (0..before.len()).map(|_| before[rng.below(before.len())]).collect();
            let rn: Vec<f64> = (0..now.len()).map(|_| now[rng.below(now.len())]).collect();
            let mb = median(&rb)?;
            (mb > 0.0).then(|| median(&rn).map(|mn| mn / mb)).flatten()
        })
        .collect();
    draws.sort_by(f64::total_cmp);
    let q = |p: f64| draws[((draws.len() - 1) as f64 * p).round() as usize];
    let (low, high) = if draws.is_empty() {
        (n / b, n / b)
    } else {
        (q(0.05), q(0.95))
    };
    Some(Ratio {
        now: n,
        before: b,
        ratio: n / b,
        low,
        high,
        weeks_now: now.len() as u32,
        weeks_before: before.len() as u32,
    })
}

/// Weekly points per FTE-day and lines added per FTE-day (per engineer-day when FTE is unknown).
struct Weekly {
    velocity: Vec<f64>,
    volume: Vec<f64>,
    approx: bool,
}

fn weekly(days: &Days, opts: &MetricOptions, ws: &[Bucket]) -> Weekly {
    let mut out = Weekly {
        velocity: Vec::new(),
        volume: Vec::new(),
        approx: false,
    };
    for w in ws {
        let agg = aggregate(days, w.start, w.end);
        let m = metrics(&agg, opts, None, w.start, w.end);
        if let Some(Some(v)) = m.values.get("velocity") {
            out.velocity.push(*v);
        }
        let fte_days = opts.velocity.as_ref().and_then(|v| v.fte_days(w.start, w.end));
        let denominator = fte_days.unwrap_or(agg.person_days as f64);
        out.approx |= fte_days.is_none();
        let added = m.values.get("added").copied().flatten().unwrap_or(0.0);
        if denominator > 0.0 {
            out.volume.push(added / denominator);
        } else if fte_days.is_some() {
            out.volume.push(0.0);
        }
    }
    out
}

/// A pooled ratio over a window (the guardrails).
fn pooled(days: &Days, opts: &MetricOptions, w: &Window, id: &str) -> Option<f64> {
    metrics(&aggregate(days, w.from, w.to), opts, None, w.from, w.to)
        .values
        .get(id)
        .copied()
        .flatten()
}

/// `own` is the marker being compared against (not counted among the markers spanned).
fn compare(
    days: &Days,
    opts: &MetricOptions,
    timeline: &Timeline,
    own: Option<&Marker>,
    before: &[Bucket],
    now: &[Bucket],
) -> Outcome {
    let window = |ws: &[Bucket]| Window {
        from: ws[0].start,
        to: ws[ws.len() - 1].end,
        weeks: ws.len() as u32,
    };
    let (wb, wn) = (window(before), window(now));
    let (b, n) = (weekly(days, opts, before), weekly(days, opts, now));
    // An idle before-window has no baseline; its few commits would make the guardrails noise.
    let before_idle = median(&b.volume).is_none_or(|m| m <= 0.0);
    let diff = |id| {
        if before_idle {
            return None;
        }
        Some(pooled(days, opts, &wn, id)? - pooled(days, opts, &wb, id)?)
    };
    Outcome::Ok {
        before_idle,
        velocity: ratio(&b.velocity, &n.velocity),
        volume: ratio(&b.volume, &n.volume),
        approx: b.approx || n.approx,
        rework_pts: diff("rework_pct"),
        tests_pts: diff("test_discipline_pct"),
        crosses_phase: timeline.phase_at(wb.from) != timeline.phase_at(wn.to),
        spans_markers: timeline
            .markers
            .iter()
            .filter(|m| m.date > wb.from && m.date <= wn.to && Some(*m) != own)
            .map(|m| m.name.clone())
            .collect(),
        before: wb,
        now: wn,
    }
}

/// The comparisons for a range ending on `end`, over data from `start` (`range_start`) to `as_of`.
pub fn speed_ups(
    days: &Days,
    opts: &MetricOptions,
    timeline: &Timeline,
    start: NaiveDate,
    end: NaiveDate,
    as_of: NaiveDate,
) -> SpeedUps {
    let end = end.min(as_of);
    let all = complete_weeks(start, end);
    let last = |n: u32| &all[all.len() - n as usize..];

    let phase = match timeline.phase_at(end) {
        Some(p) => {
            let since = complete_weeks(p.from.max(start), end);
            let outcome = match window_size(since.len() / 2, since.len() / 2) {
                Some(w) => compare(days, opts, timeline, None, &since[..w as usize], last(w)),
                None => Outcome::TooEarly {
                    weeks: since.len() as u32,
                    since: p.from,
                },
            };
            Comparison {
                label: p.name.clone(),
                date: p.from,
                source: Source::Configured,
                outcome,
            }
        }
        None => {
            // No phases: the last 8 weeks against the 12 before them.
            let w = 8usize;
            let outcome = if all.len() >= w + 4 {
                let now = &all[all.len() - w..];
                let before = &all[all.len().saturating_sub(w + FALLBACK_WEEKS as usize)..all.len() - w];
                compare(days, opts, timeline, None, before, now)
            } else {
                Outcome::TooEarly {
                    weeks: all.len() as u32,
                    since: start,
                }
            };
            Comparison {
                label: format!("the {FALLBACK_WEEKS} weeks before"),
                date: all
                    .len()
                    .checked_sub(w + FALLBACK_WEEKS as usize)
                    .map_or(start, |i| all[i].start),
                source: Source::Fallback,
                outcome,
            }
        }
    };

    let markers = timeline
        .ai_markers()
        .into_iter()
        .filter(|(m, _)| m.date <= end)
        .map(|(m, source)| {
            // The marker's own week belongs to neither side.
            let before = complete_weeks(start, m.date - Span::new(1));
            let before: Vec<_> = before.into_iter().filter(|w| w.end < m.date).collect();
            let after: Vec<_> = all.iter().filter(|w| w.start > m.date).cloned().collect();
            let outcome = match window_size(before.len(), after.len()) {
                Some(w) => compare(
                    days,
                    opts,
                    timeline,
                    Some(&m),
                    &before[before.len() - w as usize..],
                    last(w),
                ),
                None => Outcome::TooEarly {
                    weeks: after.len() as u32,
                    since: m.date,
                },
            };
            Comparison {
                label: m.name,
                date: m.date,
                source,
                outcome,
            }
        })
        .collect();

    SpeedUps { end, phase, markers }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::velocity::{FtePeriod, VelocityOptions};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    /// One commit each weekday from `from` to `to`: `lines` added, `points` scored.
    fn work(days: &mut Days, from: &str, to: &str, lines: u64, points: u32) {
        let mut x = d(from);
        while x <= d(to) {
            if chrono::Datelike::weekday(&x).num_days_from_monday() < 5 {
                let day = days.entry(x).or_default();
                day.human.commits += 1;
                day.human.added += lines;
                day.human.sizes.push(lines);
                day.person_days += 1;
                *day.person_commits.entry("p1".into()).or_default() += 1;
                day.velocity.points += points;
                day.velocity.packages += 1;
            }
            x = x.succ_opt().unwrap();
        }
    }

    fn opts() -> MetricOptions {
        MetricOptions {
            breadth_roles: vec![pm_classify::Role::Backend],
            ai_attribution: true,
            velocity: Some(VelocityOptions {
                since: d("2025-01-01"),
                as_of: d("2025-12-31"),
                fte: vec![FtePeriod {
                    from: None,
                    to: None,
                    fte: 1.0,
                }],
            }),
            ..Default::default()
        }
    }

    fn ok(c: &Comparison) -> (&Option<Ratio>, &Option<Ratio>, &Window, &Window, bool) {
        match &c.outcome {
            Outcome::Ok {
                velocity,
                volume,
                before,
                now,
                crosses_phase,
                ..
            } => (velocity, volume, before, now, *crosses_phase),
            o => panic!("expected a comparison, got {o:?}"),
        }
    }

    #[test]
    fn compares_with_the_start_of_the_current_phase() {
        let mut days = Days::new();
        // Phase from Mon 2025-03-03: 2 points a day for 8 weeks, then 6 points and 3× the lines.
        work(&mut days, "2025-03-03", "2025-04-27", 100, 2);
        work(&mut days, "2025-04-28", "2025-06-22", 300, 6);
        let t = Timeline {
            phases: vec![
                Phase {
                    name: "maintenance".into(),
                    from: d("2025-01-01"),
                },
                Phase {
                    name: "development".into(),
                    from: d("2025-03-03"),
                },
            ],
            ..Default::default()
        };
        let s = speed_ups(&days, &opts(), &t, d("2025-01-01"), d("2025-06-22"), d("2025-12-31"));
        assert_eq!(s.phase.label, "development");
        let (velocity, volume, before, now, crosses) = ok(&s.phase);
        assert_eq!(
            (before.from, before.to, before.weeks),
            (d("2025-03-03"), d("2025-04-27"), 8)
        );
        assert_eq!((now.from, now.to), (d("2025-04-28"), d("2025-06-22")));
        assert_eq!(velocity.as_ref().unwrap().ratio, 3.0);
        assert_eq!(volume.as_ref().unwrap().ratio, 3.0);
        assert!(!velocity.as_ref().unwrap().unclear());
        assert!(!crosses);
        assert!(s.markers.is_empty());
    }

    #[test]
    fn short_phases_use_four_weeks_then_say_too_early() {
        let mut days = Days::new();
        work(&mut days, "2025-01-06", "2025-06-22", 100, 2);
        let phase = |from| Timeline {
            phases: vec![Phase {
                name: "development".into(),
                from: d(from),
            }],
            ..Default::default()
        };
        // 10 complete weeks: two windows of 4.
        let s = speed_ups(
            &days,
            &opts(),
            &phase("2025-04-14"),
            d("2025-01-01"),
            d("2025-06-22"),
            d("2025-12-31"),
        );
        assert_eq!(ok(&s.phase).2.weeks, 4);
        // 5 complete weeks: too early.
        let s = speed_ups(
            &days,
            &opts(),
            &phase("2025-05-19"),
            d("2025-01-01"),
            d("2025-06-22"),
            d("2025-12-31"),
        );
        assert_eq!(
            s.phase.outcome,
            Outcome::TooEarly {
                weeks: 5,
                since: d("2025-05-19")
            }
        );
    }

    #[test]
    fn without_phases_the_weeks_before_stand_in() {
        let mut days = Days::new();
        work(&mut days, "2025-01-06", "2025-06-22", 100, 2);
        let s = speed_ups(
            &days,
            &opts(),
            &Timeline::default(),
            d("2025-01-01"),
            d("2025-06-22"),
            d("2025-12-31"),
        );
        assert_eq!(s.phase.source, Source::Fallback);
        let (velocity, _, before, now, _) = ok(&s.phase);
        assert_eq!((before.weeks, now.weeks), (12, 8));
        assert_eq!(velocity.as_ref().unwrap().ratio, 1.0);
        assert!(velocity.as_ref().unwrap().unclear(), "steady work: no clear change");
    }

    #[test]
    fn ai_markers_compare_before_and_after_and_flag_a_phase_change() {
        let mut days = Days::new();
        work(&mut days, "2025-01-06", "2025-03-09", 100, 2);
        work(&mut days, "2025-03-10", "2025-06-22", 200, 4);
        let adoption = Adoption {
            repo: "app".into(),
            date: d("2025-03-12"),
            files: vec!["CLAUDE.md".into()],
            sha: "abc".into(),
        };
        let mut t = Timeline {
            ai_adoption: AiAdoption {
                first: Some(adoption.clone()),
                repos: vec![adoption],
            },
            ..Default::default()
        };
        // Detected: stands in until configured.
        let s = speed_ups(&days, &opts(), &t, d("2025-01-01"), d("2025-06-22"), d("2025-12-31"));
        assert_eq!(s.markers.len(), 1);
        assert_eq!(
            (s.markers[0].source, s.markers[0].label.as_str()),
            (Source::Detected, "CLAUDE.md added")
        );
        let (velocity, _, before, now, crosses) = ok(&s.markers[0]);
        // Marker week (10–16 March) on neither side; 8 weeks before it, the last 8 after.
        assert_eq!((before.from, before.to), (d("2025-01-13"), d("2025-03-09")));
        assert_eq!(now.to, d("2025-06-22"));
        assert_eq!(velocity.as_ref().unwrap().ratio, 2.0);
        assert!(!crosses);

        // Configured markers win, latest first; a phase change between the windows is flagged.
        t.markers = vec![
            Marker {
                name: "first repo".into(),
                date: d("2025-03-12"),
                kind: Some("ai".into()),
            },
            Marker {
                name: "holidays".into(),
                date: d("2025-04-01"),
                kind: None,
            },
            Marker {
                name: "main repos".into(),
                date: d("2025-04-16"),
                kind: Some("ai".into()),
            },
        ];
        t.phases = vec![Phase {
            name: "development".into(),
            from: d("2025-03-10"),
        }];
        let s = speed_ups(&days, &opts(), &t, d("2025-01-01"), d("2025-06-22"), d("2025-12-31"));
        let names: Vec<_> = s.markers.iter().map(|m| (m.label.as_str(), m.source)).collect();
        assert_eq!(
            names,
            [("main repos", Source::Configured), ("first repo", Source::Configured)]
        );
        assert!(ok(&s.markers[1]).4, "before-window in no phase, now in development");
        let spanned = |c: &Comparison| match &c.outcome {
            Outcome::Ok { spans_markers, .. } => spans_markers.clone(),
            o => panic!("{o:?}"),
        };
        // "first repo" spans the holiday and the later AI step, never itself.
        assert_eq!(spanned(&s.markers[1]), ["holidays", "main repos"]);
        // "main repos" compares 17 Feb – 13 Apr with the last 8 weeks: both other markers lie between.
        assert_eq!(spanned(&s.markers[0]), ["first repo", "holidays"]);
    }

    #[test]
    fn an_idle_before_window_has_no_ratios_or_guardrails() {
        let mut days = Days::new();
        // Two stray commits in eight quiet weeks, then steady work after the marker.
        work(&mut days, "2025-01-15", "2025-01-15", 50, 1);
        work(&mut days, "2025-03-05", "2025-03-05", 2000, 5);
        work(&mut days, "2025-03-17", "2025-06-22", 300, 4);
        let t = Timeline {
            markers: vec![Marker {
                name: "AI".into(),
                date: d("2025-03-12"),
                kind: Some("ai".into()),
            }],
            ..Default::default()
        };
        let s = speed_ups(&days, &opts(), &t, d("2025-01-01"), d("2025-06-22"), d("2025-12-31"));
        match &s.markers[0].outcome {
            Outcome::Ok {
                before_idle,
                velocity,
                volume,
                rework_pts,
                tests_pts,
                ..
            } => {
                assert!(before_idle);
                assert_eq!((velocity, volume), (&None, &None));
                assert_eq!((rework_pts, tests_pts), (&None, &None));
            }
            o => panic!("{o:?}"),
        }
        // Steady work before: not idle.
        let p = speed_ups(
            &days,
            &opts(),
            &Timeline::default(),
            d("2025-01-01"),
            d("2025-06-22"),
            d("2025-12-31"),
        );
        assert!(matches!(p.phase.outcome, Outcome::Ok { before_idle: false, .. }));
    }

    #[test]
    fn medians_and_intervals() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(&[4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(&[]), None);
        let r = ratio(&[1.0, 1.0, 1.0, 1.0], &[2.0, 2.0, 2.0, 2.0]).unwrap();
        assert_eq!((r.ratio, r.low, r.high), (2.0, 2.0, 2.0));
        assert!(ratio(&[0.0, 0.0], &[1.0]).is_none(), "nothing before: no ratio");
        let noisy = ratio(&[1.0, 3.0, 0.5, 2.0, 1.5, 4.0], &[1.2, 2.5, 0.8, 3.0, 1.0, 2.2]).unwrap();
        assert!(noisy.low < 1.0 && noisy.high > 1.0, "{noisy:?}");
    }
}
