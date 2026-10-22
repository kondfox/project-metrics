//! FTE from a time-tracking export (spec §10.2): developer hours per day, read from Google Sheets.
//!
//! The export is a Drive folder whose `months` subfolder holds one spreadsheet per month, each
//! with a sheet of booking rows (date, email, project, hours). Only developers' hours count: a
//! declared role wins, else a person is a developer from `min_commits` commits in the window.
//! Everyone else's hours are overhead (spec §4.2) and only counted here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{Datelike, NaiveDate, Weekday};
use pm_config::{Fte, Identity, Timesheet};
use pm_metrics::velocity::FtePeriod;
use serde::{Deserialize, Serialize};

/// One row of the export, already filtered to the project's tracked ids.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Booking {
    pub date: NaiveDate,
    pub email: String,
    pub project: String,
    pub hours: f64,
}

/// What [`developer_fte`] found, for the collect report.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    /// One period per run of equal days from the first booking to `as_of`.
    pub periods: Vec<FtePeriod>,
    pub developers: BTreeSet<String>,
    pub developer_hours: f64,
    /// People whose hours are overhead, and their hours.
    pub overhead: BTreeMap<String, f64>,
    /// Emails with hours that match no commit author and have no declared role.
    pub unmatched: BTreeSet<String>,
}

/// `h:mm` (any number of hour digits) or a decimal number of hours.
pub fn parse_hours(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some((h, m)) = s.split_once(':') {
        let (h, m) = (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?);
        return (m < 60).then(|| f64::from(h) + f64::from(m) / 60.0);
    }
    s.replace(',', ".").parse::<f64>().ok().filter(|h| *h >= 0.0)
}

/// Lower-case name tokens with accents folded, for matching `jane.doe@…` to "Doe Jáne".
fn tokens(s: &str) -> BTreeSet<String> {
    s.chars()
        .map(fold)
        .collect::<String>()
        .split(|c: char| !c.is_ascii_alphabetic())
        .filter(|t| t.len() > 1)
        .map(str::to_string)
        .collect()
}

fn fold(c: char) -> char {
    let c = c.to_lowercase().next().unwrap_or(c);
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' | 'į' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ő' | 'ø' | 'ō' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ű' | 'ū' | 'ů' | 'ų' => 'u',
        'ý' | 'ÿ' => 'y',
        'ç' | 'ć' | 'č' => 'c',
        'ñ' | 'ń' | 'ň' => 'n',
        'š' | 'ś' | 'ş' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ř' => 'r',
        'ď' => 'd',
        'ť' => 't',
        'ł' | 'ľ' | 'ĺ' => 'l',
        'ß' => 's',
        c => c,
    }
}

/// The commit author an export email belongs to: the `[people]` person it is listed for, else the
/// one author whose name has the same tokens as the email's local part.
fn person_for(email: &str, identity: &Identity, authors: &BTreeMap<String, u32>) -> Option<String> {
    if let Some(p) = identity.mapped(email) {
        return Some(p.to_string());
    }
    let want = tokens(email.split('@').next().unwrap_or(email));
    let mut hits = authors.keys().filter(|a| !want.is_empty() && tokens(a) == want);
    match (hits.next(), hits.next()) {
        (Some(a), None) => Some(a.clone()),
        _ => None,
    }
}

/// The declared role on `d`: the latest entry for this email or person that has started.
fn role_on<'a>(fte: &'a Fte, email: &str, person: Option<&str>, d: NaiveDate) -> Option<&'a str> {
    fte.roles
        .iter()
        .filter(|r| {
            let who = r.who.trim();
            who.eq_ignore_ascii_case(email) || person.is_some_and(|p| who.eq_ignore_ascii_case(p))
        })
        .filter(|r| r.from.is_none_or(|f| f <= d))
        .max_by_key(|r| r.from)
        .map(|r| r.role.as_str())
}

/// Weekend hours count on the Friday before: FTE-days only add up business days (spec §4).
fn business_day(d: NaiveDate) -> NaiveDate {
    match d.weekday() {
        Weekday::Sat => d - chrono::Days::new(1),
        Weekday::Sun => d - chrono::Days::new(2),
        _ => d,
    }
}

/// Developer FTE per day (hours ÷ 8) from the 1st of the first booking's month to `as_of` (so that
/// month is whole, not unknown for its first days); bookings after `as_of` are ignored. `commits` is commits per person (spec §1.3: bots and externals already out) over the
/// measured range.
pub fn developer_fte(
    bookings: &[Booking],
    fte: &Fte,
    identity: &Identity,
    commits: &BTreeMap<String, u32>,
    as_of: NaiveDate,
) -> Outcome {
    let min = fte.min_commits.unwrap_or(Fte::DEFAULT_MIN_COMMITS);
    let mut out = Outcome::default();
    let mut by_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let mut persons: BTreeMap<&str, Option<String>> = BTreeMap::new();
    for b in bookings.iter().filter(|b| b.date <= as_of) {
        let person = persons
            .entry(b.email.as_str())
            .or_insert_with(|| person_for(&b.email, identity, commits))
            .clone();
        let developer = match role_on(fte, &b.email, person.as_deref(), b.date) {
            Some(r) => r.eq_ignore_ascii_case("dev"),
            None => person.as_ref().and_then(|p| commits.get(p)).is_some_and(|n| *n >= min),
        };
        *by_day.entry(business_day(b.date)).or_default() += if developer { b.hours } else { 0.0 };
        if developer {
            out.developers.insert(person.clone().unwrap_or_else(|| b.email.clone()));
            out.developer_hours += b.hours;
        } else {
            *out.overhead
                .entry(person.clone().unwrap_or_else(|| b.email.clone()))
                .or_default() += b.hours;
            if person.is_none() && role_on(fte, &b.email, None, b.date).is_none() {
                out.unmatched.insert(b.email.clone());
            }
        }
    }
    let Some(first) = by_day.keys().next().and_then(|d| d.with_day(1)) else {
        return out;
    };
    let mut d = first;
    while d <= as_of {
        let value = by_day.get(&d).copied().unwrap_or(0.0) / 8.0;
        match out.periods.last_mut() {
            Some(p) if p.fte == value && p.to.and_then(|t| t.succ_opt()) == Some(d) => p.to = Some(d),
            _ => out.periods.push(FtePeriod {
                from: Some(d),
                to: Some(d),
                fte: value,
            }),
        }
        let Some(next) = d.succ_opt() else { break };
        d = next;
    }
    out
}

/// The export's daily FTE over `[fte] value` / `periods`, which fill only the days the export
/// doesn't cover (e.g. before time was tracked). Later periods win, so the export goes last.
pub fn with_fallback(fte: &Fte, from_export: Vec<FtePeriod>) -> Vec<FtePeriod> {
    let mut out: Vec<FtePeriod> = fte
        .value
        .map(|v| FtePeriod {
            from: None,
            to: None,
            fte: v,
        })
        .into_iter()
        .chain(fte.periods.iter().map(|p| FtePeriod {
            from: Some(p.from),
            to: p.to,
            fte: p.fte,
        }))
        .collect();
    out.extend(from_export);
    out
}

/// Rows of a sheet (first row = header) to bookings of the tracked `projects`.
pub fn bookings_from_rows(rows: &[Vec<String>], ts: &Timesheet, projects: &[String]) -> Result<Vec<Booking>> {
    let Some((header, rows)) = rows.split_first() else {
        return Ok(Vec::new());
    };
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h.trim() == name)
            .ok_or_else(|| anyhow!("no column `{name}` (columns: {})", header.join(", ")))
    };
    let c = &ts.columns;
    let (date, email, project, hours) = (col(&c.date)?, col(&c.email)?, col(&c.project)?, col(&c.hours)?);
    let cell = |r: &Vec<String>, i: usize| r.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut out = Vec::new();
    for r in rows {
        let p = cell(r, project);
        if !projects.iter().any(|x| x.trim() == p) {
            continue;
        }
        let (d, h) = (cell(r, date), cell(r, hours));
        out.push(Booking {
            date: NaiveDate::parse_from_str(&d, "%Y-%m-%d").with_context(|| format!("date `{d}`"))?,
            email: cell(r, email).to_lowercase(),
            project: p,
            hours: parse_hours(&h).ok_or_else(|| anyhow!("hours `{h}`"))?,
        });
    }
    Ok(out)
}

/// Read every monthly spreadsheet and keep the project's bookings; the copy in `cache` is
/// refreshed, and used (with a warning from the caller) when the export can't be read.
pub fn load(ts: &Timesheet, projects: &[String], cache: &Path) -> Result<(Vec<Booking>, Option<String>)> {
    match fetch(ts, projects) {
        Ok(b) => {
            if let Some(dir) = cache.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(cache, serde_json::to_string(&b)?)?;
            Ok((b, None))
        }
        Err(e) => {
            let Ok(text) = std::fs::read_to_string(cache) else {
                return Err(e.context("reading the time-tracking export (and no earlier copy)"));
            };
            Ok((
                serde_json::from_str(&text)?,
                Some(format!(
                    "time-tracking export not read ({e:#}): using the copy from the last collect"
                )),
            ))
        }
    }
}

fn fetch(ts: &Timesheet, projects: &[String]) -> Result<Vec<Booking>> {
    let token = token(ts)?;
    let months = list(&token, &ts.folder)?
        .into_iter()
        .find(|f| f.name == ts.months && f.mime.ends_with(".folder"))
        .ok_or_else(|| anyhow!("no `{}` folder in the time-tracking export", ts.months))?;
    let mut sheets: Vec<DriveFile> = list(&token, &months.id)?
        .into_iter()
        .filter(|f| f.mime.ends_with(".spreadsheet"))
        .collect();
    sheets.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::new();
    for s in &sheets {
        let url = format!(
            "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
            s.id,
            encode(&ts.tab)
        );
        let v = get_json(&token, &url).with_context(|| format!("sheet `{}` of `{}`", ts.tab, s.name))?;
        let rows: Vec<Vec<String>> = v["values"]
            .as_array()
            .map(|rs| {
                rs.iter()
                    .map(|r| {
                        r.as_array()
                            .map(|cs| cs.iter().map(|c| c.as_str().unwrap_or_default().to_string()).collect())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.extend(bookings_from_rows(&rows, ts, projects).with_context(|| format!("`{}`", s.name))?);
    }
    Ok(out)
}

fn token(ts: &Timesheet) -> Result<String> {
    let mut cmd = Command::new("gcloud");
    cmd.args(["auth", "print-access-token"])
        .arg(format!("--impersonate-service-account={}", ts.impersonate))
        .arg("--scopes=https://www.googleapis.com/auth/drive.readonly,https://www.googleapis.com/auth/spreadsheets.readonly");
    if let Some(a) = &ts.account {
        cmd.arg(format!("--account={a}"));
    }
    let out = cmd
        .output()
        .context("running gcloud (is the Google Cloud CLI installed?)")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err.lines().find(|l| l.starts_with("ERROR")).unwrap_or(err.trim());
        bail!("gcloud could not get a token for {}: {line}", ts.impersonate);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

struct DriveFile {
    id: String,
    name: String,
    mime: String,
}

fn list(token: &str, folder: &str) -> Result<Vec<DriveFile>> {
    let q = encode(&format!("'{folder}' in parents and trashed=false"));
    let url = format!(
        "https://www.googleapis.com/drive/v3/files?q={q}&pageSize=1000&fields=files(id,name,mimeType)&supportsAllDrives=true&includeItemsFromAllDrives=true"
    );
    let v = get_json(token, &url)?;
    Ok(v["files"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .map(|f| DriveFile {
                    id: f["id"].as_str().unwrap_or_default().to_string(),
                    name: f["name"].as_str().unwrap_or_default().to_string(),
                    mime: f["mimeType"].as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default())
}

fn get_json(token: &str, url: &str) -> Result<serde_json::Value> {
    let mut res = ureq::get(url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| anyhow!("{e}"))?;
    let text = res.body_mut().read_to_string()?;
    Ok(serde_json::from_str(&text)?)
}

/// Percent-encode a query value or path segment.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pm_config::{FteRole, FteSource, People, TimesheetColumns};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn fte(roles: Vec<FteRole>) -> Fte {
        Fte {
            source: FteSource::Timesheet,
            value: None,
            periods: Vec::new(),
            file: None,
            projects: vec!["7".into()],
            min_commits: None,
            roles,
            timesheet: None,
        }
    }

    fn b(date: &str, email: &str, hours: f64) -> Booking {
        Booking {
            date: d(date),
            email: email.into(),
            project: "7".into(),
            hours,
        }
    }

    fn commits(xs: &[(&str, u32)]) -> BTreeMap<String, u32> {
        xs.iter().map(|(p, n)| (p.to_string(), *n)).collect()
    }

    #[test]
    fn hours_formats() {
        assert_eq!(parse_hours("7:30"), Some(7.5));
        assert_eq!(parse_hours("1234:05"), Some(1234.0 + 5.0 / 60.0));
        assert_eq!(parse_hours("6,25"), Some(6.25));
        assert_eq!(parse_hours("8"), Some(8.0));
        assert_eq!(parse_hours("7:75"), None);
        assert_eq!(parse_hours(""), None);
    }

    #[test]
    fn emails_match_authors_by_folded_name() {
        let id = Identity::default();
        let authors = commits(&[("Jáne Doé", 30), ("Sam Roe", 2)]);
        assert_eq!(
            person_for("jane.doe@example.com", &id, &authors),
            Some("Jáne Doé".into())
        );
        assert_eq!(
            person_for("doe.jane@example.com", &id, &authors),
            Some("Jáne Doé".into())
        );
        assert_eq!(person_for("max.moe@example.com", &id, &authors), None);
    }

    #[test]
    fn people_aliases_win_over_names() {
        let mut people = People::default();
        people.persons.insert("Jane Doe".into(), vec!["jd@example.com".into()]);
        let id = Identity::new(&people);
        assert_eq!(
            person_for("jd@example.com", &id, &BTreeMap::new()),
            Some("Jane Doe".into())
        );
    }

    #[test]
    fn only_developers_count() {
        let id = Identity::default();
        let authors = commits(&[("Jane Doe", 40), ("Sam Roe", 3)]);
        let bookings = [
            b("2026-08-03", "jane.doe@example.com", 8.0),
            b("2026-08-03", "sam.roe@example.com", 8.0),
            b("2026-08-03", "max.moe@example.com", 4.0),
            b("2026-08-04", "jane.doe@example.com", 4.0),
        ];
        let o = developer_fte(&bookings, &fte(vec![]), &id, &authors, d("2026-08-05"));
        assert_eq!(o.developers, BTreeSet::from(["Jane Doe".to_string()]));
        assert_eq!(o.developer_hours, 12.0);
        assert_eq!(o.overhead.get("Sam Roe"), Some(&8.0));
        assert_eq!(o.unmatched, BTreeSet::from(["max.moe@example.com".to_string()]));
        let at = |s: &str| {
            o.periods
                .iter()
                .find(|p| p.from <= Some(d(s)) && Some(d(s)) <= p.to)
                .map(|p| p.fte)
        };
        assert_eq!(at("2026-08-03"), Some(1.0));
        assert_eq!(at("2026-08-04"), Some(0.5));
        assert_eq!(at("2026-08-05"), Some(0.0), "a day without bookings is 0, not unknown");
        assert_eq!(at("2026-08-01"), Some(0.0), "the first booking's month is whole");
        assert_eq!(at("2026-07-31"), None, "before that month FTE is unknown");
    }

    #[test]
    fn declared_role_wins_over_commits_and_carries_a_date() {
        let id = Identity::default();
        let authors = commits(&[("Jane Doe", 40), ("Sam Roe", 25)]);
        let roles = vec![
            FteRole {
                who: "Sam Roe".into(),
                role: "qa".into(),
                from: None,
            },
            FteRole {
                who: "sam.roe@example.com".into(),
                role: "dev".into(),
                from: Some(d("2026-08-04")),
            },
        ];
        let bookings = [
            b("2026-08-03", "sam.roe@example.com", 8.0),
            b("2026-08-04", "sam.roe@example.com", 8.0),
            b("2026-08-04", "max.moe@example.com", 8.0),
        ];
        let mut f = fte(roles);
        f.roles.push(FteRole {
            who: "max.moe@example.com".into(),
            role: "dev".into(),
            from: None,
        });
        let o = developer_fte(&bookings, &f, &id, &authors, d("2026-08-04"));
        assert_eq!(
            o.overhead.get("Sam Roe"),
            Some(&8.0),
            "QA on the 3rd despite 25 commits"
        );
        assert_eq!(
            o.developer_hours, 16.0,
            "Sam from the 4th, and Max declared dev without commits"
        );
        assert!(o.unmatched.is_empty());
    }

    #[test]
    fn static_periods_fill_only_where_the_export_is_silent() {
        let id = Identity::default();
        let authors = commits(&[("Jane Doe", 40)]);
        let bookings = [b("2026-08-03", "jane.doe@example.com", 4.0)];
        let mut f = fte(vec![]);
        f.periods = vec![pm_config::FtePeriod {
            from: d("2026-01-01"),
            to: Some(d("2026-08-31")),
            fte: 1.0,
        }];
        let o = developer_fte(&bookings, &f, &id, &authors, d("2026-08-05"));
        let opts = pm_metrics::velocity::VelocityOptions {
            since: d("2026-01-01"),
            as_of: d("2026-08-05"),
            fte: with_fallback(&f, o.periods),
        };
        assert_eq!(
            opts.fte_on(d("2026-07-15")),
            Some(1.0),
            "before the export: the static value"
        );
        assert_eq!(
            opts.fte_on(d("2026-08-03")),
            Some(0.5),
            "the export wins where it has a day"
        );
        assert_eq!(
            opts.fte_on(d("2026-08-04")),
            Some(0.0),
            "an empty export day is 0, not the fallback"
        );
        assert_eq!(opts.fte_on(d("2025-12-31")), None);
    }

    #[test]
    fn bookings_after_as_of_are_ignored() {
        let id = Identity::default();
        let authors = commits(&[("Jane Doe", 40)]);
        let bookings = [
            b("2026-08-03", "jane.doe@example.com", 8.0),
            b("2026-08-10", "jane.doe@example.com", 8.0),
        ];
        let o = developer_fte(&bookings, &fte(vec![]), &id, &authors, d("2026-08-05"));
        assert_eq!(o.developer_hours, 8.0);
        assert_eq!(o.periods.last().and_then(|p| p.to), Some(d("2026-08-05")));
    }

    #[test]
    fn weekend_hours_count_on_friday() {
        let id = Identity::default();
        let authors = commits(&[("Jane Doe", 40)]);
        let bookings = [
            b("2026-08-07", "jane.doe@example.com", 4.0),
            b("2026-08-08", "jane.doe@example.com", 2.0),
            b("2026-08-09", "jane.doe@example.com", 2.0),
        ];
        let o = developer_fte(&bookings, &fte(vec![]), &id, &authors, d("2026-08-10"));
        let at = |s: &str| {
            o.periods
                .iter()
                .find(|p| p.from <= Some(d(s)) && Some(d(s)) <= p.to)
                .map(|p| p.fte)
        };
        assert_eq!(at("2026-08-07"), Some(1.0));
        assert_eq!(at("2026-08-08"), Some(0.0));
    }

    #[test]
    fn rows_by_header_name() {
        let ts = Timesheet {
            folder: "f".into(),
            months: "Months".into(),
            tab: "Rows".into(),
            columns: TimesheetColumns {
                date: "Date".into(),
                email: "Email".into(),
                project: "Project".into(),
                hours: "Hours".into(),
            },
            impersonate: "sa@example.iam.gserviceaccount.com".into(),
            account: None,
        };
        let row = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let rows = vec![
            row(&["Email", "Date", "Project", "Name", "Hours"]),
            row(&["Jane.Doe@example.com", "2026-08-03", "7", "x", "7:30"]),
            row(&["sam.roe@example.com", "2026-08-03", "8", "x", "8:00"]),
        ];
        let got = bookings_from_rows(&rows, &ts, &["7".into()]).unwrap();
        assert_eq!(got, vec![b("2026-08-03", "jane.doe@example.com", 7.5)]);
        let bad = vec![row(&["Email", "Date"])];
        assert!(bookings_from_rows(&bad, &ts, &["7".into()]).is_err());
    }
}
