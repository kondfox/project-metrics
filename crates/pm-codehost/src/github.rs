//! GitHub: merged pull requests over GraphQL with their submitted reviews (spec §10.4), and a
//! reader for `gh pr list --json` exports (the prototype's input, used by the parity harness).

use std::sync::Arc;

use serde_json::{Value, json};

use crate::http::{Http, Retry};
use crate::{
    Account, CodeHost, CodeHostError, FileChange, MergedPr, PrCommit, Query, Result, Review, Selection, Sink,
    parse_time,
};

const PAGE: u32 = 25;

const QUERY: &str = r#"query($owner: String!, $name: String!, $cursor: String, $order: IssueOrderField!, $files: Boolean!, $commits: Boolean!) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: MERGED, orderBy: {field: $order, direction: DESC}, first: PAGE, after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number createdAt mergedAt updatedAt additions deletions
        author { login __typename ... on User { name } }
        mergeCommit { oid }
        commits(first: 100) @include(if: $commits) { nodes { commit { oid messageHeadline authoredDate author { email } } } }
        files(first: 100) @include(if: $files) { nodes { path additions } }
        reviews(first: 100) { nodes { author { login __typename ... on User { name } } submittedAt } }
      }
    }
  }
}"#;

pub struct GitHub {
    /// `https://api.github.com/graphql`, or `<base>/api/graphql` for GitHub Enterprise Server.
    endpoint: String,
    token: String,
    http: Arc<dyn Http>,
    pub retry: Retry,
}

impl GitHub {
    /// `base`: `None` (or `https://github.com`) for github.com, else the Enterprise Server URL.
    pub fn new(base: Option<&str>, token: String, http: Arc<dyn Http>) -> GitHub {
        let endpoint = match base.map(|b| b.trim_end_matches('/')) {
            None | Some("https://github.com") | Some("https://api.github.com") => {
                "https://api.github.com/graphql".into()
            }
            Some(b) => format!("{b}/api/graphql"),
        };
        GitHub {
            endpoint,
            token,
            http,
            retry: Retry::default(),
        }
    }
}

/// A GraphQL connection (`{nodes: […]}`) or a gh export's flat array.
fn items(v: &Value) -> &[Value] {
    v.get("nodes")
        .and_then(Value::as_array)
        .or_else(|| v.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn account(v: &Value) -> Option<Account> {
    Some(Account {
        login: v["login"].as_str()?.to_string(),
        name: v["name"].as_str().filter(|n| !n.trim().is_empty()).map(String::from),
        bot: v["__typename"] == "Bot" || v["is_bot"].as_bool().unwrap_or(false),
    })
}

/// gh writes some numbers as strings.
fn number(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}

/// One PR node (GraphQL or gh export). `None` if it isn't merged.
fn pr(url: &str, n: &Value) -> Result<Option<MergedPr>> {
    let Some(merged) = n["mergedAt"].as_str() else {
        return Ok(None);
    };
    let mut reviews = Vec::new();
    for r in items(&n["reviews"]) {
        // Pending reviews have no submission time.
        if let (Some(by), Some(at)) = (account(&r["author"]), r["submittedAt"].as_str()) {
            reviews.push(Review {
                by,
                at: parse_time(url, at)?,
            });
        }
    }
    let files = (!n["files"].is_null()).then(|| {
        items(&n["files"])
            .iter()
            .map(|f| FileChange {
                path: f["path"].as_str().unwrap_or_default().to_string(),
                additions: number(&f["additions"]).unwrap_or(0),
            })
            .collect()
    });
    let commits = if n["commits"].is_null() {
        None
    } else {
        let mut out = Vec::new();
        for c in items(&n["commits"]) {
            let c = &c["commit"];
            let Some(sha) = c["oid"].as_str() else { continue };
            out.push(PrCommit {
                sha: sha.to_string(),
                subject: c["messageHeadline"].as_str().unwrap_or_default().to_string(),
                author_email: c["author"]["email"]
                    .as_str()
                    .filter(|e| !e.is_empty())
                    .map(String::from),
                authored_at: c["authoredDate"].as_str().map(|t| parse_time(url, t)).transpose()?,
            });
        }
        Some(out)
    };
    Ok(Some(MergedPr {
        id: format!("#{}", number(&n["number"]).unwrap_or(0)),
        author: account(&n["author"]),
        created_at: parse_time(url, n["createdAt"].as_str().unwrap_or_default())?,
        merged_at: parse_time(url, merged)?,
        updated_at: n["updatedAt"].as_str().map(|u| parse_time(url, u)).transpose()?,
        additions: number(&n["additions"]),
        deletions: number(&n["deletions"]),
        files,
        reviews,
        landed: n["mergeCommit"]["oid"].as_str().map(String::from).into_iter().collect(),
        commits,
    }))
}

/// The PRs of a `gh pr list --state merged --json number,author,createdAt,mergedAt,additions,deletions,files,reviews`
/// export.
pub fn from_gh_export(v: &Value) -> Result<Vec<MergedPr>> {
    let mut out = Vec::new();
    for n in items(v) {
        if let Some(p) = pr("gh export", n)? {
            out.push(p);
        }
    }
    Ok(out)
}

impl CodeHost for GitHub {
    fn merged_prs(&self, q: &Query, sink: &mut dyn Sink) -> Result<()> {
        let (owner, name) = q.project.split_once('/').ok_or_else(|| CodeHostError::Parse {
            url: self.endpoint.clone(),
            message: format!("code_host_id `{}` is not owner/repo", q.project),
        })?;
        let (order, field) = match q.selection {
            Selection::UpdatedSince => ("UPDATED_AT", "updatedAt"),
            Selection::CreatedSince => ("CREATED_AT", "createdAt"),
        };
        let query = QUERY.replace("PAGE", &PAGE.to_string());
        let auth = format!("bearer {}", self.token);
        let mut cursor = Value::Null;
        loop {
            let body = json!({
                "query": query,
                "variables": {"owner": owner, "name": name, "cursor": cursor, "order": order, "files": q.files,
                              "commits": q.commits},
            })
            .to_string();
            let res = self.retry.run(&self.endpoint, || {
                self.http.post(
                    &self.endpoint,
                    &[("Authorization", &auth), ("Content-Type", "application/json")],
                    &body,
                )
            })?;
            let v: Value = serde_json::from_str(&res.body).map_err(|e| CodeHostError::Parse {
                url: self.endpoint.clone(),
                message: e.to_string(),
            })?;
            if let Some(errs) = v["errors"].as_array().filter(|e| !e.is_empty()) {
                let msg: Vec<&str> = errs.iter().filter_map(|e| e["message"].as_str()).collect();
                return Err(CodeHostError::Parse {
                    url: format!("{} ({})", self.endpoint, q.project),
                    message: msg.join("; "),
                });
            }
            let conn = &v["data"]["repository"]["pullRequests"];
            let mut reached_since = false;
            let mut listed = 0;
            for n in items(conn) {
                let t = n[field].as_str().map(|s| parse_time(&self.endpoint, s)).transpose()?;
                if t.is_some_and(|t| t < q.since) {
                    reached_since = true;
                    break;
                }
                listed += 1;
                match pr(&self.endpoint, n)? {
                    Some(p) if q.until.is_some_and(|u| p.merged_at.date_naive() > u) => sink.skipped(),
                    Some(p) if !sink.is_current(&p.id, p.updated_at.as_ref()) => sink.pr(p),
                    _ => sink.skipped(),
                }
            }
            sink.listed(listed);
            let more = conn["pageInfo"]["hasNextPage"].as_bool().unwrap_or(false);
            if reached_since || !more {
                return Ok(());
            }
            cursor = conn["pageInfo"]["endCursor"].clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Collect;
    use crate::tests::FakeHttp;

    fn node(number: u64, updated: &str, reviews: Value) -> Value {
        json!({"number": number, "createdAt": "2025-02-03T09:00:00Z", "mergedAt": "2025-02-04T10:00:00Z",
               "updatedAt": updated, "additions": 10, "deletions": 2,
               "author": {"login": "ann", "__typename": "User"}, "reviews": {"nodes": reviews}})
    }

    #[test]
    fn pages_until_the_since_bound() {
        let http = FakeHttp::default()
            .with_post(
                "\"cursor\":null",
                json!({"data": {"repository": {"pullRequests": {
                    "pageInfo": {"hasNextPage": true, "endCursor": "c1"},
                    "nodes": [node(9, "2025-03-01T00:00:00Z", json!([
                        {"author": {"login": "bob", "__typename": "User"}, "submittedAt": "2025-02-03T12:00:00Z"},
                        {"author": {"login": "ci", "__typename": "Bot"}, "submittedAt": "2025-02-03T12:00:00Z"},
                        {"author": {"login": "cy", "__typename": "User"}, "submittedAt": null}]))]}}}}),
            )
            .with_post(
                "\"cursor\":\"c1\"",
                json!({"data": {"repository": {"pullRequests": {
                    "pageInfo": {"hasNextPage": true, "endCursor": "c2"},
                    "nodes": [node(8, "2025-02-10T00:00:00Z", json!([])), node(7, "2024-12-31T23:59:59Z", json!([]))]}}}}),
            );
        let gh = GitHub::new(None, "tok".into(), Arc::new(http.clone()));
        let mut sink = Collect::default();
        let q = Query {
            project: "acme/shop",
            since: "2025-01-01T00:00:00Z".parse().unwrap(),
            selection: Selection::UpdatedSince,
            until: None,
            files: false,
            commits: false,
        };
        gh.merged_prs(&q, &mut sink).unwrap();
        let ids: Vec<&str> = sink.prs.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["#9", "#8"]);
        assert_eq!(sink.prs[0].reviews.len(), 2);
        assert!(sink.prs[0].reviews[1].by.bot);
        assert_eq!(sink.prs[0].additions, Some(10));
        assert!(http.saw_header("Authorization", "bearer tok"));
        assert_eq!(http.urls()[0], "https://api.github.com/graphql");
    }

    #[test]
    fn reads_commits_and_the_merge_commit() {
        let mut n = node(5, "2025-02-04T10:00:00Z", json!([]));
        n["mergeCommit"] = json!({"oid": "m5"});
        n["commits"] = json!({"nodes": [
            {"commit": {"oid": "c1", "messageHeadline": "add form", "authoredDate": "2025-02-03T09:10:00+01:00",
                        "author": {"email": "ann@work.example"}}},
            {"commit": {"oid": "c2", "messageHeadline": "review fixes", "authoredDate": "2025-02-03T15:00:00Z",
                        "author": {"email": null}}}]});
        let http = FakeHttp::default().with_post(
            "\"commits\":true",
            json!({"data": {"repository": {"pullRequests": {
                "pageInfo": {"hasNextPage": false, "endCursor": null}, "nodes": [n]}}}}),
        );
        let gh = GitHub::new(None, "tok".into(), Arc::new(http));
        let mut sink = Collect::default();
        let q = Query {
            project: "acme/shop",
            since: "2025-01-01T00:00:00Z".parse().unwrap(),
            selection: Selection::UpdatedSince,
            until: None,
            files: false,
            commits: true,
        };
        gh.merged_prs(&q, &mut sink).unwrap();
        let p = &sink.prs[0];
        assert_eq!(p.landed, ["m5"]);
        let c = p.commits.as_ref().unwrap();
        assert_eq!((c[0].sha.as_str(), c[0].subject.as_str()), ("c1", "add form"));
        assert_eq!(c[0].author_email.as_deref(), Some("ann@work.example"));
        assert_eq!(c[1].author_email, None);
        // Not asked for: not fetched, not an empty list.
        assert_eq!(
            pr("x", &node(6, "2025-02-04T10:00:00Z", json!([])))
                .unwrap()
                .unwrap()
                .commits,
            None
        );
    }

    #[test]
    fn graphql_errors_are_errors() {
        let http = FakeHttp::default().with_post(
            "",
            json!({"data": {"repository": null}, "errors": [{"message": "Could not resolve to a Repository"}]}),
        );
        let gh = GitHub::new(Some("https://github.example.com/"), "t".into(), Arc::new(http.clone()));
        let q = Query {
            project: "acme/none",
            since: "2025-01-01T00:00:00Z".parse().unwrap(),
            selection: Selection::UpdatedSince,
            until: None,
            files: true,
            commits: false,
        };
        let err = gh.merged_prs(&q, &mut Collect::default()).unwrap_err();
        assert!(err.to_string().contains("Could not resolve"), "{err}");
        assert_eq!(http.urls()[0], "https://github.example.com/api/graphql");
    }

    #[test]
    fn reads_gh_exports() {
        let v = json!([
            {"number": "12", "author": {"login": "ann", "is_bot": false}, "createdAt": "2025-02-03T09:00:00Z",
             "mergedAt": "2025-02-04T10:00:00Z", "additions": "5", "deletions": "1",
             "files": [{"path": "web/a.tsx", "additions": 5, "deletions": 1}],
             "reviews": [{"author": {"login": "bob"}, "submittedAt": "2025-02-03T10:00:00Z", "state": "APPROVED"}]},
            {"number": 13, "author": null, "createdAt": "2025-02-03T09:00:00Z", "mergedAt": null, "reviews": []}
        ]);
        let prs = from_gh_export(&v).unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(prs[0].id, "#12");
        assert_eq!(prs[0].files.as_ref().unwrap()[0].additions, 5);
        assert_eq!((prs[0].additions, prs[0].deletions), (Some(5), Some(1)));
        assert_eq!(prs[0].reviews[0].by.login, "bob");
    }
}
