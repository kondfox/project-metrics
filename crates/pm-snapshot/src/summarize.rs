//! Raw tool results → one repo's [`RepoSummary`]: the classification rules of spec §7.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use pm_classify::Classifier;
use pm_metrics::snapshot::{RepoSummary, SastSummary, SecretsSummary, Severity, VulnPackage, VulnSummary};
use regex::Regex;

use crate::raw::{RawScan, SecretHit};

/// Paths whose findings don't count: tests, fixtures, mocks and seed data.
static SECURITY_TEST_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^|/)(tests?|__tests__|spec|e2e|fixtures?|mocks?|seed)(/|$)|\.(test|spec)\.\w+$").unwrap()
});

/// Documentation and samples: a secret there is never high confidence.
static NOISE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(^|/)docs?/|(^|/)prd/|\.md$|(^|/)README|\.example$|\.sample$|(^|/)(sample|example)[^/]*$").unwrap()
});

/// Key material or a provider-specific token format: near-zero false positives (spec §7.2).
const HIGH_CONFIDENCE_RULES: &[&str] = &[
    "private-key",
    "pkcs12",
    "aws-",
    "gitlab-",
    "github-",
    "azure-ad-client-secret",
    "gcp-service-account",
    "slack-",
    "stripe-",
    "sendgrid-",
    "twilio-",
    "openai-",
    "anthropic-",
    "kubernetes-secret",
    "npm-access-token",
    "digitalocean-",
    "heroku-",
    "hashicorp-",
    "jfrog-",
    "doppler-",
    "postman-",
];

/// A human verdict that removes a live secret from the gate (spec §10.1 `[[secrets_triage]]`).
#[derive(Clone, Debug)]
pub struct Triage {
    pub repo: String,
    pub file: String,
    pub rule: Option<String>,
}

pub struct SecurityRules {
    /// `None`: the prototype's test-path rule only (parity harness).
    pub classifier: Option<Classifier>,
    pub triage: Vec<Triage>,
}

impl SecurityRules {
    /// Spec: the spec's test rules, plus fixtures, mocks and seed folders.
    pub fn is_test_path(&self, path: &str) -> bool {
        SECURITY_TEST_PATH.is_match(path) || self.classifier.as_ref().is_some_and(|c| c.matches_test(path))
    }

    fn is_high_confidence(&self, hit: &SecretHit) -> bool {
        !self.is_test_path(&hit.file)
            && !NOISE_PATH.is_match(&hit.file)
            && HIGH_CONFIDENCE_RULES.iter().any(|p| hit.rule.starts_with(p))
    }

    fn triaged(&self, repo: &str, hit: &SecretHit) -> bool {
        self.triage
            .iter()
            .any(|t| t.repo == repo && t.file == hit.file && t.rule.as_ref().is_none_or(|r| *r == hit.rule))
    }
}

/// semgrep severity → high / medium / low (spec §7.4).
pub fn sast_bucket(severity: &str) -> &'static str {
    match severity.to_uppercase().as_str() {
        "ERROR" | "HIGH" | "CRITICAL" => "high",
        "WARNING" | "MEDIUM" => "medium",
        _ => "low",
    }
}

pub fn summarize(repo: &str, raw: &RawScan, rules: &SecurityRules) -> RepoSummary {
    let sast = raw.sast.as_ref().map(|findings| {
        let mut s = SastSummary::default();
        for f in findings.iter().filter(|f| !rules.is_test_path(&f.file)) {
            match sast_bucket(&f.severity) {
                "high" => s.high += 1,
                "medium" => s.medium += 1,
                _ => s.low += 1,
            }
            *s.keys.entry(format!("{}\t{}", f.rule, f.file)).or_insert(0) += 1;
        }
        s
    });
    let secrets = raw.secrets.as_ref().map(|hits| {
        let mut s = SecretsSummary::default();
        for h in hits {
            if rules.is_high_confidence(h) && !rules.triaged(repo, h) {
                s.high += 1;
            } else {
                s.low += 1;
            }
        }
        s
    });
    let vulns = raw.lockfiles.as_ref().map(|scans| {
        let mut v = VulnSummary::default();
        for p in scans.iter().flat_map(|l| &l.packages) {
            v.total_packages += 1;
            for (id, sev) in &p.vulns {
                let e = v.advisories.entry(id.clone()).or_insert(*sev);
                *e = (*e).max(*sev);
            }
            if !p.vulns.is_empty() {
                v.packages.push(VulnPackage {
                    name: p.name.clone(),
                    version: p.version.clone(),
                    count: p.vulns.len() as u64,
                    max_severity: p.vulns.iter().map(|(_, s)| *s).max().unwrap_or(Severity::Unknown),
                });
            }
        }
        v
    });
    RepoSummary {
        repo: repo.to_string(),
        sha: raw.sha.clone(),
        code: raw.scc.map(|s| s.code),
        complexity: raw.scc.map(|s| s.complexity),
        dup_lines: raw.duplication.map(|d| d.duplicated_lines),
        dup_total: raw.duplication.map(|d| d.lines),
        kloc_all: raw.kloc_all,
        sast,
        secrets,
        suppressions: raw.suppressions,
        vulns,
    }
}

/// `kloc_all` from a whole-tree scc count: KLOC to 0.1, as the SAST density uses it (ties to
/// even on the exact value, like the prototype's `round(x, 1)`).
pub fn kloc(code: u64) -> f64 {
    format!("{:.1}", code as f64 / 1000.0).parse().expect("formatted float")
}

/// SAST findings per rule and file, for drill-downs.
pub fn by_rule(raw: &RawScan, rules: &SecurityRules) -> BTreeMap<String, u64> {
    let mut out = BTreeMap::new();
    for f in raw.sast.iter().flatten().filter(|f| !rules.is_test_path(&f.file)) {
        *out.entry(f.rule.clone()).or_insert(0) += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::{LockfileScan, OsvPackage, SastFinding};

    fn hit(rule: &str, file: &str) -> SecretHit {
        SecretHit {
            rule: rule.into(),
            file: file.into(),
            line: 1,
        }
    }

    fn finding(sev: &str, file: &str) -> SastFinding {
        SastFinding {
            rule: "detect-eval".into(),
            severity: sev.into(),
            file: file.into(),
            line: 1,
            cwe: None,
        }
    }

    #[test]
    fn classifies_secrets_and_findings() {
        let rules = SecurityRules {
            classifier: Some(Classifier::default()),
            triage: vec![Triage {
                repo: "api".into(),
                file: "certs/dev.pem".into(),
                rule: None,
            }],
        };
        let raw = RawScan {
            sha: "abc".into(),
            secrets: Some(vec![
                hit("aws-access-token", "config/prod.env"),
                hit("private-key", "certs/dev.pem"),
                hit("private-key", "test/fixtures/key.pem"),
                hit("generic-api-key", "src/client.ts"),
                hit("github-pat", "docs/setup.md"),
                hit("stripe-access-token", "internal/pay_test.go"),
            ]),
            sast: Some(vec![
                finding("ERROR", "src/a.js"),
                finding("WARNING", "src/a.js"),
                finding("INFO", "src/b.js"),
                finding("ERROR", "e2e/login.ts"),
                finding("ERROR", "src/a.spec.ts"),
                finding("ERROR", "pkg/x_test.go"),
            ]),
            lockfiles: Some(vec![LockfileScan {
                path: "package-lock.json".into(),
                packages: vec![
                    OsvPackage {
                        name: "a".into(),
                        version: "1".into(),
                        vulns: vec![("X-1".into(), Severity::High), ("X-2".into(), Severity::Low)],
                    },
                    OsvPackage {
                        name: "b".into(),
                        version: "2".into(),
                        vulns: vec![("X-1".into(), Severity::Critical)],
                    },
                    OsvPackage {
                        name: "c".into(),
                        version: "3".into(),
                        vulns: vec![],
                    },
                ],
            }]),
            ..Default::default()
        };
        let s = summarize("api", &raw, &rules);
        // Only the AWS token outside tests, docs and triage is high.
        assert_eq!(s.secrets, Some(SecretsSummary { high: 1, low: 5 }));
        let sast = s.sast.unwrap();
        assert_eq!((sast.high, sast.medium, sast.low), (1, 1, 1));
        assert_eq!(sast.keys["detect-eval\tsrc/a.js"], 2);
        let v = s.vulns.unwrap();
        assert_eq!(v.advisories["X-1"], Severity::Critical);
        assert_eq!(v.total_packages, 3);
        assert_eq!(v.packages.len(), 2);
        assert_eq!(v.packages[0].max_severity, Severity::High);
    }

    #[test]
    fn prototype_rules_miss_go_tests() {
        let proto = SecurityRules {
            classifier: None,
            triage: vec![],
        };
        let spec = SecurityRules {
            classifier: Some(Classifier::default()),
            triage: vec![],
        };
        assert!(!proto.is_test_path("pkg/x_test.go"));
        assert!(spec.is_test_path("pkg/x_test.go"));
        assert!(proto.is_test_path("db/seed/users.sql"));
    }

    #[test]
    fn kloc_rounding() {
        assert_eq!(kloc(182_749), 182.7);
        assert_eq!(kloc(50), 0.1);
        assert_eq!(kloc(0), 0.0);
    }
}
