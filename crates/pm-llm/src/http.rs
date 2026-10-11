//! The HTTP seam for the API providers: a real client ([`UreqHttp`]) and retries for rate limits and
//! overload. Tests use fakes.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::{Error, Result};

#[derive(Clone, Debug, Default)]
pub struct HttpResponse {
    pub status: u16,
    /// Lower-cased header names.
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

/// One POST. `Err` is a transport failure (no response); any status is an `Ok`.
pub trait Http: Send + Sync {
    fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> std::result::Result<HttpResponse, String>;
}

pub struct UreqHttp {
    agent: ureq::Agent,
}

impl UreqHttp {
    pub fn new(timeout: Duration) -> UreqHttp {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .user_agent(concat!("pmx/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        UreqHttp { agent }
    }
}

impl Http for UreqHttp {
    fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> std::result::Result<HttpResponse, String> {
        let mut req = self.agent.post(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let mut res = req.send(body).map_err(|e| e.to_string())?;
        let status = res.status().as_u16();
        let headers = res
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_lowercase(), v.to_str().ok()?.to_string())))
            .collect();
        let body = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok(HttpResponse { status, headers, body })
    }
}

/// Retries 429, 529 (overloaded), 5xx and dropped connections with backoff; `retry-after` wins
/// (capped at a minute).
#[derive(Clone, Copy, Debug)]
pub struct Retry {
    pub attempts: u32,
    pub base: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Retry {
            attempts: 5,
            base: Duration::from_secs(2),
        }
    }
}

impl Retry {
    /// POST until a final answer; a non-2xx final answer is an error naming the provider.
    pub fn post(&self, http: &dyn Http, who: &str, url: &str, headers: &[(&str, &str)], body: &str) -> Result<String> {
        let mut wait = self.base;
        let mut attempt = 1;
        loop {
            let outcome = http.post(url, headers, body);
            let again = match &outcome {
                Ok(r) => matches!(r.status, 429 | 500 | 502 | 503 | 504 | 529),
                Err(_) => true,
            };
            if !again || attempt >= self.attempts {
                return match outcome {
                    Ok(r) if (200..300).contains(&r.status) => Ok(r.body),
                    Ok(r) => Err(Error::Provider(format!(
                        "{who}: HTTP {}: {}",
                        r.status,
                        r.body.chars().take(300).collect::<String>()
                    ))),
                    Err(e) => Err(Error::Provider(format!("{who}: {e}"))),
                };
            }
            let pause = outcome
                .as_ref()
                .ok()
                .and_then(|r| r.headers.get("retry-after"))
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|s| Duration::from_secs(s.min(60)))
                .unwrap_or(wait);
            std::thread::sleep(pause);
            wait *= 2;
            attempt += 1;
        }
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::sync::Mutex;

    use super::*;

    /// URL, headers and body of one request.
    pub type Recorded = (String, Vec<(String, String)>, serde_json::Value);

    /// Answers each POST from a queue (or with the last answer once empty) and records requests.
    #[derive(Default)]
    pub struct FakeHttp {
        pub answers: Mutex<Vec<(u16, String)>>,
        pub requests: Mutex<Vec<Recorded>>,
    }

    impl FakeHttp {
        pub fn new(answers: Vec<(u16, String)>) -> FakeHttp {
            FakeHttp {
                answers: Mutex::new(answers),
                ..Default::default()
            }
        }
    }

    impl Http for FakeHttp {
        fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> std::result::Result<HttpResponse, String> {
            self.requests.lock().unwrap().push((
                url.to_string(),
                headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                serde_json::from_str(body).unwrap(),
            ));
            let mut a = self.answers.lock().unwrap();
            let (status, body) = if a.len() > 1 { a.remove(0) } else { a[0].clone() };
            Ok(HttpResponse {
                status,
                body,
                ..Default::default()
            })
        }
    }

    #[test]
    fn retries_overload_then_succeeds() {
        let h = FakeHttp::new(vec![(529, "overloaded".into()), (200, "ok".into())]);
        let r = Retry {
            attempts: 3,
            base: Duration::ZERO,
        };
        assert_eq!(r.post(&h, "x", "u", &[], "{}").unwrap(), "ok");
        let h = FakeHttp::new(vec![(400, "bad schema".into())]);
        let e = r.post(&h, "x", "u", &[], "{}").unwrap_err().to_string();
        assert!(e.contains("HTTP 400: bad schema"), "{e}");
        assert_eq!(h.requests.lock().unwrap().len(), 1, "a 400 is final");
    }
}
