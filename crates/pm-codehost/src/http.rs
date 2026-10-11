//! The HTTP seam: a real client ([`UreqHttp`]) and the retry policy shared by the adapters.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::{CodeHostError, Result};

#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    /// Lower-cased header names.
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// One HTTP exchange. `Err` is a transport failure (no response); any status is an `Ok`.
pub trait Http: Send + Sync {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> std::result::Result<Response, String>;
    fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> std::result::Result<Response, String>;
}

/// The real client.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl Default for UreqHttp {
    fn default() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(concat!("pmx/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        UreqHttp { agent }
    }
}

fn read(mut res: ureq::http::Response<ureq::Body>) -> std::result::Result<Response, String> {
    let status = res.status().as_u16();
    let headers = res
        .headers()
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_lowercase(), v.to_str().ok()?.to_string())))
        .collect();
    let body = res
        .body_mut()
        .with_config()
        .limit(256 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| e.to_string())?;
    Ok(Response { status, headers, body })
}

impl Http for UreqHttp {
    fn get(&self, url: &str, headers: &[(&str, &str)]) -> std::result::Result<Response, String> {
        let mut req = self.agent.get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        read(req.call().map_err(|e| e.to_string())?)
    }

    fn post(&self, url: &str, headers: &[(&str, &str)], body: &str) -> std::result::Result<Response, String> {
        let mut req = self.agent.post(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        read(req.send(body).map_err(|e| e.to_string())?)
    }
}

/// Retries for rate limits, server errors and dropped connections.
#[derive(Clone, Copy, Debug)]
pub struct Retry {
    pub attempts: u32,
    /// First wait; doubles each time. A `Retry-After` header wins (capped at a minute).
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

fn retryable(r: &Response) -> bool {
    matches!(r.status, 429 | 500 | 502 | 503 | 504)
        // GitHub's secondary rate limit is a 403 with Retry-After.
        || (r.status == 403 && r.header("retry-after").is_some())
}

impl Retry {
    /// Run `send` until it gives a final answer. Non-2xx answers become errors that name `url`.
    pub fn run(&self, url: &str, send: impl Fn() -> std::result::Result<Response, String>) -> Result<Response> {
        let mut wait = self.base;
        let mut attempt = 1;
        loop {
            let outcome = send();
            let again = match &outcome {
                Ok(r) => retryable(r),
                Err(_) => true,
            };
            if !again || attempt >= self.attempts {
                return match outcome {
                    Ok(r) if (200..300).contains(&r.status) => Ok(r),
                    Ok(r) => Err(CodeHostError::Status {
                        url: url.to_string(),
                        status: r.status,
                        message: status_hint(r.status, &r.body),
                    }),
                    Err(e) => Err(CodeHostError::Http {
                        url: url.to_string(),
                        message: e,
                    }),
                };
            }
            let pause = outcome
                .as_ref()
                .ok()
                .and_then(|r| r.header("retry-after"))
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|s| Duration::from_secs(s.min(60)))
                .unwrap_or(wait);
            std::thread::sleep(pause);
            wait *= 2;
            attempt += 1;
        }
    }
}

fn status_hint(status: u16, body: &str) -> String {
    let snippet: String = body.chars().take(200).collect();
    match status {
        401 => format!("the token was rejected ({snippet})"),
        403 => format!("access denied: does the token have read access to this project? ({snippet})"),
        404 => format!("not found: check `code_host_id` and the token's access ({snippet})"),
        _ => snippet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn retries_then_gives_up_with_the_status() {
        let calls = Mutex::new(0);
        let r = Retry {
            attempts: 3,
            base: Duration::ZERO,
        };
        let out = r.run("u", || {
            *calls.lock().unwrap() += 1;
            Ok(Response {
                status: 503,
                ..Default::default()
            })
        });
        assert_eq!(*calls.lock().unwrap(), 3);
        assert!(matches!(out, Err(CodeHostError::Status { status: 503, .. })));

        let calls = Mutex::new(0);
        let out = r.run("u", || {
            let mut n = calls.lock().unwrap();
            *n += 1;
            if *n == 1 {
                Err("reset".into())
            } else {
                Ok(Response {
                    status: 200,
                    body: "ok".into(),
                    ..Default::default()
                })
            }
        });
        assert_eq!(out.unwrap().body, "ok");
        // A 404 is final.
        let out = r.run("u", || {
            Ok(Response {
                status: 404,
                ..Default::default()
            })
        });
        assert!(matches!(out, Err(CodeHostError::Status { status: 404, .. })));
    }
}
