//! GitLab: `GET /projects/:id/merge_requests?state=merged` plus each MR's notes (spec §10.4). A
//! review is a non-system note or an approval system note (spec §8).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::http::{Http, Retry};
use crate::{
    Account, CodeHost, CodeHostError, FileChange, MergedPr, Query, Result, Review, Selection, Sink, format_since,
    parse_time,
};

pub struct GitLab {
    /// `https://gitlab.example.com/api/v4`
    api: String,
    token: String,
    http: Arc<dyn Http>,
    pub retry: Retry,
    /// MRs whose notes are fetched at the same time.
    pub workers: usize,
}

impl GitLab {
    /// `base` is the instance URL, e.g. `https://gitlab.com`.
    pub fn new(base: &str, token: String, http: Arc<dyn Http>) -> GitLab {
        GitLab {
            api: format!("{}/api/v4", base.trim_end_matches('/')),
            token,
            http,
            retry: Retry::default(),
            workers: 4,
        }
    }

    fn get(&self, path: &str) -> Result<(Value, Option<String>)> {
        let url = format!("{}{path}", self.api);
        let res = self
            .retry
            .run(&url, || self.http.get(&url, &[("PRIVATE-TOKEN", &self.token)]))?;
        let v: Value = serde_json::from_str(&res.body).map_err(|e| CodeHostError::Parse {
            url: url.clone(),
            message: e.to_string(),
        })?;
        let next = res
            .header("x-next-page")
            .filter(|s| !s.trim().is_empty())
            .map(String::from);
        Ok((v, next))
    }

    /// Every page of a list endpoint (`path` has a query string already), as the prototype read
    /// it: `per_page=100&page=N`, following `X-Next-Page`.
    fn pages(&self, path: &str, mut each: impl FnMut(Vec<Value>) -> Result<()>) -> Result<()> {
        let mut page = "1".to_string();
        loop {
            let (v, next) = self.get(&format!("{path}&per_page=100&page={page}"))?;
            let Value::Array(items) = v else { break };
            if items.is_empty() {
                break;
            }
            each(items)?;
            match next {
                Some(n) => page = n,
                None => break,
            }
        }
        Ok(())
    }

    fn reviews(&self, pid: &str, iid: u64) -> Result<Vec<Review>> {
        let path = format!("/projects/{pid}/merge_requests/{iid}/notes?sort=asc&order_by=created_at");
        let mut out = Vec::new();
        self.pages(&path, |notes| {
            for n in notes {
                let Some(by) = account(&n["author"]) else {
                    continue;
                };
                let review = if n["system"].as_bool().unwrap_or(false) {
                    n["body"]
                        .as_str()
                        .is_some_and(|b| b.to_lowercase().contains("approved this merge request"))
                } else {
                    true
                };
                if review {
                    out.push(Review {
                        by,
                        at: parse_time(&path, n["created_at"].as_str().unwrap_or_default())?,
                    });
                }
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Added lines per changed file, from the MR's diffs (`/diffs`, GitLab ≥ 15.7; else `/changes`).
    fn files(&self, pid: &str, iid: u64) -> Result<Vec<FileChange>> {
        let mut out = Vec::new();
        let mut add = |d: &Value| {
            let path = d["new_path"].as_str().unwrap_or_default().to_string();
            let additions = d["diff"]
                .as_str()
                .unwrap_or_default()
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++ "))
                .count() as u64;
            out.push(FileChange { path, additions });
        };
        match self.pages(
            &format!("/projects/{pid}/merge_requests/{iid}/diffs?unidiff=false"),
            |ds| {
                ds.iter().for_each(&mut add);
                Ok(())
            },
        ) {
            Err(CodeHostError::Status { status: 404, .. }) => {
                let (v, _) = self.get(&format!("/projects/{pid}/merge_requests/{iid}/changes"))?;
                v["changes"].as_array().into_iter().flatten().for_each(&mut add);
            }
            other => other?,
        }
        Ok(out)
    }
}

/// A project id as a path segment: numeric ids as they are, `group/project` URL-encoded.
fn account(v: &Value) -> Option<Account> {
    Some(Account {
        login: v["username"].as_str()?.to_string(),
        name: v["name"].as_str().filter(|n| !n.trim().is_empty()).map(String::from),
        bot: v["bot"].as_bool().unwrap_or(false),
    })
}

fn project_segment(id: &str) -> String {
    id.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

struct Listed {
    iid: u64,
    author: Option<Account>,
    created_at: String,
    merged_at: String,
    updated_at: Option<String>,
}

impl CodeHost for GitLab {
    fn merged_prs(&self, q: &Query, sink: &mut dyn Sink) -> Result<()> {
        let pid = project_segment(q.project);
        let filter = match q.selection {
            Selection::UpdatedSince => "updated_after",
            Selection::CreatedSince => "created_after",
        };
        let path = format!(
            "/projects/{pid}/merge_requests?state=merged&{filter}={}",
            format_since(&q.since)
        );
        let mut todo = Vec::new();
        self.pages(&path, |mrs| {
            sink.listed(mrs.len() as u64);
            for m in mrs {
                let (Some(iid), Some(merged)) = (m["iid"].as_u64(), m["merged_at"].as_str()) else {
                    sink.skipped();
                    continue;
                };
                if let Some(until) = q.until {
                    if parse_time(&path, merged)?.date_naive() > until {
                        sink.skipped();
                        continue;
                    }
                }
                let updated = m["updated_at"].as_str().map(String::from);
                let updated_t = updated.as_deref().map(|u| parse_time(&path, u)).transpose()?;
                if sink.is_current(&format!("!{iid}"), updated_t.as_ref()) {
                    sink.skipped();
                    continue;
                }
                todo.push(Listed {
                    iid,
                    author: account(&m["author"]),
                    created_at: m["created_at"].as_str().unwrap_or_default().to_string(),
                    merged_at: merged.to_string(),
                    updated_at: updated,
                });
            }
            Ok(())
        })?;

        // Notes (and files) per MR, a few at a time.
        let next = AtomicUsize::new(0);
        let failed = AtomicBool::new(false);
        let first_error: Mutex<Option<CodeHostError>> = Mutex::new(None);
        let sink = Mutex::new(sink);
        std::thread::scope(|s| {
            for _ in 0..self.workers.clamp(1, todo.len().max(1)) {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::SeqCst);
                        if i >= todo.len() || failed.load(Ordering::SeqCst) {
                            break;
                        }
                        let l = &todo[i];
                        let pr = (|| -> Result<MergedPr> {
                            Ok(MergedPr {
                                id: format!("!{}", l.iid),
                                author: l.author.clone(),
                                created_at: parse_time(&path, &l.created_at)?,
                                merged_at: parse_time(&path, &l.merged_at)?,
                                updated_at: l.updated_at.as_deref().map(|u| parse_time(&path, u)).transpose()?,
                                additions: None,
                                deletions: None,
                                files: if q.files { Some(self.files(&pid, l.iid)?) } else { None },
                                reviews: self.reviews(&pid, l.iid)?,
                            })
                        })();
                        match pr {
                            Ok(pr) => sink.lock().unwrap_or_else(|e| e.into_inner()).pr(pr),
                            Err(e) => {
                                failed.store(true, Ordering::SeqCst);
                                first_error.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert(e);
                            }
                        }
                    }
                });
            }
        });
        match first_error.into_inner().unwrap_or_else(|e| e.into_inner()) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Collect;
    use crate::http::Response;
    use crate::tests::FakeHttp;
    use serde_json::json;

    fn host(http: FakeHttp) -> GitLab {
        let mut g = GitLab::new("https://gitlab.example.com/", "t0k".into(), Arc::new(http));
        g.retry.base = std::time::Duration::ZERO;
        g
    }

    fn since() -> chrono::DateTime<chrono::Utc> {
        "2025-01-01T00:00:00Z".parse().unwrap()
    }

    #[test]
    fn lists_mrs_and_reads_reviews_from_notes() {
        let base = "https://gitlab.example.com/api/v4/projects/7/merge_requests";
        let list = format!("{base}?state=merged&updated_after=2025-01-01T00:00:00Z&per_page=100");
        let http = FakeHttp::default()
            .with(
                &format!("{list}&page=1"),
                Response {
                    status: 200,
                    headers: [("x-next-page".to_string(), "2".to_string())].into(),
                    body: json!([{"iid": 1, "author": {"username": "ann"}, "created_at": "2025-02-03T09:00:00.000Z",
                                  "merged_at": "2025-02-04T10:30:00.000Z", "updated_at": "2025-02-04T10:30:00.000Z"},
                                 {"iid": 2, "author": {"username": "bob"}, "created_at": "2025-02-03T09:00:00Z",
                                  "merged_at": null}])
                    .to_string(),
                },
            )
            .with_json(
                &format!("{list}&page=2"),
                json!([{"iid": 3, "author": {"username": "bob"}, "created_at": "2025-03-01T08:00:00+01:00",
                        "merged_at": "2025-03-02T08:00:00+01:00", "updated_at": "2025-03-02T08:00:00+01:00"}]),
            )
            .with_json(
                &format!("{base}/1/notes?sort=asc&order_by=created_at&per_page=100&page=1"),
                json!([
                    {"author": {"username": "bob"}, "system": true, "body": "added 1 commit", "created_at": "2025-02-03T09:30:00Z"},
                    {"author": {"username": "bob"}, "system": false, "body": "nit", "created_at": "2025-02-03T11:00:00Z"},
                    {"author": {"username": "cy"}, "system": true, "body": "approved this merge request", "created_at": "2025-02-04T09:00:00Z"},
                    {"author": {"username": "ann"}, "system": false, "body": "thanks", "created_at": "2025-02-04T09:10:00Z"}
                ]),
            )
            .with_json(
                &format!("{base}/3/notes?sort=asc&order_by=created_at&per_page=100&page=1"),
                json!([]),
            );
        let mut sink = Collect::default();
        let q = Query {
            project: "7",
            since: since(),
            selection: Selection::UpdatedSince,
            until: None,
            files: false,
        };
        host(http.clone()).merged_prs(&q, &mut sink).unwrap();
        assert_eq!((sink.listed, sink.skipped), (3, 1));
        sink.prs.sort_by(|a, b| a.id.cmp(&b.id));
        let mr = &sink.prs[0];
        assert_eq!(mr.id, "!1");
        assert_eq!(mr.author.as_ref().unwrap().login, "ann");
        let who: Vec<&str> = mr.reviews.iter().map(|r| r.by.login.as_str()).collect();
        // The author's own note stays; the metrics drop it.
        assert_eq!(who, ["bob", "cy", "ann"]);
        assert_eq!(sink.prs[1].merged_at.to_rfc3339(), "2025-03-02T08:00:00+01:00");
        assert!(http.saw_header("PRIVATE-TOKEN", "t0k"));
    }

    #[test]
    fn per_file_repos_get_added_lines_per_file() {
        let base = "https://gitlab.example.com/api/v4/projects/acme%2Fshop/merge_requests";
        let http = FakeHttp::default()
            .with_json(
                &format!("{base}?state=merged&created_after=2025-01-01T00:00:00Z&per_page=100&page=1"),
                json!([{"iid": 5, "author": {"username": "ann"}, "created_at": "2025-02-03T09:00:00Z",
                        "merged_at": "2025-02-04T10:30:00Z"}]),
            )
            .with_json(
                &format!("{base}/5/notes?sort=asc&order_by=created_at&per_page=100&page=1"),
                json!([]),
            )
            .with_json(
                &format!("{base}/5/diffs?unidiff=false&per_page=100&page=1"),
                json!([{"new_path": "web/App.tsx", "diff": "@@ -1 +1,3 @@\n-a\n+b\n+c\n++d\n"},
                       {"new_path": "api/x.go", "diff": "@@ -0,0 +1 @@\n+x\n"}]),
            );
        let mut sink = Collect::default();
        let q = Query {
            project: "acme/shop",
            since: since(),
            selection: Selection::CreatedSince,
            until: None,
            files: true,
        };
        host(http).merged_prs(&q, &mut sink).unwrap();
        let files = sink.prs[0].files.clone().unwrap();
        assert_eq!(
            files,
            vec![
                FileChange {
                    path: "web/App.tsx".into(),
                    additions: 3
                },
                FileChange {
                    path: "api/x.go".into(),
                    additions: 1
                }
            ]
        );
    }

    #[test]
    fn current_mrs_are_not_fetched_again() {
        struct Known(Collect);
        impl Sink for Known {
            fn listed(&mut self, n: u64) {
                self.0.listed(n)
            }
            fn is_current(&self, id: &str, _: Option<&chrono::DateTime<chrono::FixedOffset>>) -> bool {
                id == "!1"
            }
            fn pr(&mut self, pr: MergedPr) {
                self.0.pr(pr)
            }
            fn skipped(&mut self) {
                self.0.skipped()
            }
        }
        let base = "https://gitlab.example.com/api/v4/projects/7/merge_requests";
        // No notes response for !1: fetching them would fail.
        let http = FakeHttp::default().with_json(
            &format!("{base}?state=merged&updated_after=2025-01-01T00:00:00Z&per_page=100&page=1"),
            json!([{"iid": 1, "author": {"username": "ann"}, "created_at": "2025-02-03T09:00:00Z",
                    "merged_at": "2025-02-04T10:30:00Z", "updated_at": "2025-02-04T10:30:00Z"}]),
        );
        let mut sink = Known(Collect::default());
        let q = Query {
            project: "7",
            since: since(),
            selection: Selection::UpdatedSince,
            until: None,
            files: false,
        };
        host(http).merged_prs(&q, &mut sink).unwrap();
        assert_eq!((sink.0.prs.len(), sink.0.skipped), (0, 1));
    }
}
