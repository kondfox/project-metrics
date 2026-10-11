//! Code-host adapters (metrics spec §8, §10.4): every merged pull request (GitHub) or merge request
//! (GitLab) of a repo with what the PR-flow metrics need — author, created and merged times, size,
//! changed files and reviews. Adapters only fetch and normalize; who counts and what is a peer
//! review by a non-author is decided in `pm-metrics`.
//!
//! HTTP goes through the [`Http`] trait, so tests and the parity harness can replay recorded
//! responses without a network.

pub mod github;
pub mod gitlab;
pub mod http;

use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

pub use github::GitHub;
pub use gitlab::GitLab;
pub use http::{Http, Response, UreqHttp};

#[derive(Debug, thiserror::Error)]
pub enum CodeHostError {
    #[error("{url}: {message}")]
    Http { url: String, message: String },
    #[error("{url}: HTTP {status}: {message}")]
    Status { url: String, status: u16, message: String },
    #[error("{url}: unexpected response: {message}")]
    Parse { url: String, message: String },
}

pub type Result<T> = std::result::Result<T, CodeHostError>;

/// A code-host account. Bots are flagged when the host says so (GitHub's `Bot` type, gh's
/// `is_bot`); names are matched against the bot rules later (spec §1.3).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub login: String,
    /// Display name, where the host gives one (helps `pmx people` map logins to people).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bot: bool,
}

/// A submitted review: GitHub's submitted review, or on GitLab a non-system note or an approval
/// (spec §8). It may be the PR author's own; the metrics drop those.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    pub by: Account,
    pub at: DateTime<FixedOffset>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub additions: u64,
}

/// One merged PR/MR (spec §10.4). Times keep the offset the host recorded (spec §1.1: bucketed by
/// the event's date in its own timezone).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergedPr {
    /// `#123` (GitHub) or `!123` (GitLab).
    pub id: String,
    /// `None` for a deleted account.
    pub author: Option<Account>,
    pub created_at: DateTime<FixedOffset>,
    pub merged_at: DateTime<FixedOffset>,
    /// Last change on the host; incremental fetches refresh PRs updated since the last run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<FixedOffset>>,
    /// Line counts (GitHub only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additions: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletions: Option<u64>,
    /// Changed files with added lines, fetched only where the stack is per file (GitHub returns at
    /// most 100 files).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileChange>>,
    pub reviews: Vec<Review>,
}

/// Which merged PRs a fetch returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    /// Updated on or after `since`: every PR merged since then (merging updates a PR), and PRs
    /// that got new reviews. Incremental runs use the last fetch time.
    UpdatedSince,
    /// Created on or after `since` (GitLab only): the prototype's request, for the parity replay.
    CreatedSince,
}

/// One repo's fetch.
#[derive(Clone, Debug)]
pub struct Query<'a> {
    /// GitHub `owner/repo`; GitLab numeric id or `group/project` path.
    pub project: &'a str,
    pub since: DateTime<Utc>,
    pub selection: Selection,
    /// PRs merged after this date (in their recorded offset) are skipped without fetching their
    /// details: a run as of a past date.
    pub until: Option<NaiveDate>,
    /// Fetch the changed files (per-file repos, spec §8: the PR's stack is its dominant role).
    pub files: bool,
}

/// Receives a fetch's results as they arrive, so a caller can store each PR at once (an
/// interrupted run keeps what it got) and skip details it already has.
pub trait Sink: Send {
    /// More PRs were listed (the progress total).
    fn listed(&mut self, n: u64);
    /// The stored copy of this PR is current; don't fetch its details again.
    fn is_current(&self, id: &str, updated_at: Option<&DateTime<FixedOffset>>) -> bool;
    fn pr(&mut self, pr: MergedPr);
    /// A listed PR that needed no fetch (current, or not merged).
    fn skipped(&mut self);
}

/// A code host.
pub trait CodeHost: Send + Sync {
    fn merged_prs(&self, q: &Query, sink: &mut dyn Sink) -> Result<()>;
}

pub(crate) fn parse_time(url: &str, s: &str) -> Result<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(s).map_err(|e| CodeHostError::Parse {
        url: url.to_string(),
        message: format!("bad timestamp `{s}`: {e}"),
    })
}

/// A `since` bound as the hosts take it: `2024-09-01T00:00:00Z`.
pub fn format_since(t: &DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// A sink that keeps everything (tests, the parity harness).
#[derive(Default)]
pub struct Collect {
    pub prs: Vec<MergedPr>,
    pub listed: u64,
    pub skipped: u64,
}

impl Sink for Collect {
    fn listed(&mut self, n: u64) {
        self.listed += n;
    }
    fn is_current(&self, _: &str, _: Option<&DateTime<FixedOffset>>) -> bool {
        false
    }
    fn pr(&mut self, pr: MergedPr) {
        self.prs.push(pr);
    }
    fn skipped(&mut self) {
        self.skipped += 1;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use crate::http::{Http, Response};

    /// A request seen: the URL and its headers.
    type Seen = (String, Vec<(String, String)>);

    /// Canned responses: GETs by exact URL, POSTs by a substring of the body.
    #[derive(Clone, Default)]
    pub struct FakeHttp {
        get: Arc<Mutex<BTreeMap<String, Response>>>,
        post: Arc<Mutex<Vec<(String, Response)>>>,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl FakeHttp {
        pub fn with(self, url: &str, r: Response) -> Self {
            self.get.lock().unwrap().insert(url.to_string(), r);
            self
        }

        pub fn with_json(self, url: &str, v: serde_json::Value) -> Self {
            self.with(
                url,
                Response {
                    status: 200,
                    body: v.to_string(),
                    ..Default::default()
                },
            )
        }

        pub fn with_post(self, body_contains: &str, v: serde_json::Value) -> Self {
            self.post.lock().unwrap().push((
                body_contains.to_string(),
                Response {
                    status: 200,
                    body: v.to_string(),
                    ..Default::default()
                },
            ));
            self
        }

        pub fn saw_header(&self, k: &str, v: &str) -> bool {
            self.seen
                .lock()
                .unwrap()
                .iter()
                .any(|(_, hs)| hs.iter().any(|(a, b)| a == k && b == v))
        }

        pub fn urls(&self) -> Vec<String> {
            self.seen.lock().unwrap().iter().map(|(u, _)| u.clone()).collect()
        }

        fn record(&self, url: &str, headers: &[(&str, &str)]) {
            self.seen.lock().unwrap().push((
                url.to_string(),
                headers.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
            ));
        }
    }

    impl Http for FakeHttp {
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<Response, String> {
            self.record(url, headers);
            Ok(self.get.lock().unwrap().get(url).cloned().unwrap_or(Response {
                status: 404,
                body: format!("no fake for {url}"),
                ..Default::default()
            }))
        }

        fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> Result<Response, String> {
            self.record(url, headers);
            let posts = self.post.lock().unwrap();
            Ok(posts
                .iter()
                .find(|(needle, _)| body.contains(needle.as_str()))
                .map(|(_, r)| r.clone())
                .unwrap_or(Response {
                    status: 404,
                    ..Default::default()
                }))
        }
    }
}
