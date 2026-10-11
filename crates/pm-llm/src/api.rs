//! The HTTP API providers (plan §6.2): `jev` (Jev's schema as is), `openai-compatible` and
//! `anthropic` (the prompt template and a generated JSON schema).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::{Value, json};

use crate::http::{Http, Retry};
use crate::prompt::{self, TEMPLATE_VERSION};
use crate::provider::Provider;
use crate::schema::{Answer, ConfidenceSource, Question, Request, Response, Usage};
use crate::{Error, Result};

pub const JEV_URL: &str = "https://api.typesafe.ai";
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com";
/// Room for adaptive thinking plus the answer object.
const ANTHROPIC_MAX_TOKENS: u32 = 16_000;
/// Models that take `fallbacks: "default"`: a declined request is re-run server-side on another
/// model, which then shows in the response's `model` (recorded per package).
const SERVER_FALLBACK_MODELS: [&str; 4] = [
    "claude-fable-5-1",
    "claude-opus-5-5",
    "claude-opus-5",
    "claude-sonnet-5-5",
];

fn base(url: &str) -> &str {
    url.trim_end_matches('/')
}

/// Jev: the request goes through unchanged; its answers carry calibrated probabilities.
pub struct Jev {
    pub http: Arc<dyn Http>,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub retry: Retry,
}

impl Provider for Jev {
    fn identity(&self) -> String {
        format!("jev:{}", self.model)
    }

    fn local(&self) -> bool {
        false
    }

    fn classify(&self, _context: &str, req: &Request) -> Result<Response> {
        // Jev judges each question on the state alone: the rubric context is in the instructions.
        let body = json!({"model": self.model, "state": req.state, "questions": req.questions});
        let auth = format!("Bearer {}", self.api_key);
        let text = self.retry.post(
            self.http.as_ref(),
            "jev",
            &format!("{}/v1/systemone", base(&self.base_url)),
            &[("authorization", &auth), ("content-type", "application/json")],
            &body.to_string(),
        )?;
        let mut r: Response =
            serde_json::from_str(&text).map_err(|e| Error::Answer(format!("jev: not a response ({e})")))?;
        r.confidence_source = ConfidenceSource::Model;
        for (id, _) in req.questions.iter() {
            if !r.answers.contains_key(id) {
                return Err(Error::Answer(format!("jev: no answer to `{id}`")));
            }
        }
        Ok(r)
    }
}

/// The Claude API (Messages) with structured output. Raw HTTP: there is no official Rust SDK.
pub struct Anthropic {
    pub http: Arc<dyn Http>,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub effort: Option<String>,
    pub retry: Retry,
}

impl Provider for Anthropic {
    fn identity(&self) -> String {
        format!(
            "anthropic:{}:{}:{TEMPLATE_VERSION}",
            self.model,
            self.effort.as_deref().unwrap_or("default")
        )
    }

    fn local(&self) -> bool {
        false
    }

    fn classify(&self, context: &str, req: &Request) -> Result<Response> {
        let mut output = json!({"format": {"type": "json_schema", "schema": prompt::strict_schema(req)}});
        if let Some(e) = &self.effort {
            output["effort"] = json!(e);
        }
        let mut body = json!({
            "model": self.model,
            "max_tokens": ANTHROPIC_MAX_TOKENS,
            "system": prompt::SYSTEM,
            "messages": [{"role": "user", "content": prompt::render(context, req)}],
            "output_config": output,
        });
        let mut headers = vec![
            ("x-api-key", self.api_key.as_str()),
            ("anthropic-version", "2023-06-01"),
            ("content-type", "application/json"),
        ];
        if SERVER_FALLBACK_MODELS.contains(&self.model.as_str()) {
            body["fallbacks"] = json!("default");
            headers.push(("anthropic-beta", "server-side-fallback-2026-07-01"));
        }
        let text = self.retry.post(
            self.http.as_ref(),
            "anthropic",
            &format!("{}/v1/messages", base(&self.base_url)),
            &headers,
            &body.to_string(),
        )?;
        let v: Value = serde_json::from_str(&text).map_err(|e| Error::Answer(format!("anthropic: {e}")))?;
        read_anthropic(req, &v, &self.model)
    }
}

/// Read a Messages API response.
pub fn read_anthropic(req: &Request, v: &Value, model: &str) -> Result<Response> {
    match v["stop_reason"].as_str() {
        Some("refusal") => {
            let category = v["stop_details"]["category"].as_str().unwrap_or("unspecified");
            return Err(Error::Provider(format!("anthropic: the model declined ({category})")));
        }
        Some("max_tokens") => return Err(Error::Answer("anthropic: the answer was cut at max_tokens".into())),
        _ => {}
    }
    let text: String = v["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|b| b["type"] == "text")
        .filter_map(|b| b["text"].as_str())
        .collect();
    let out: Value = serde_json::from_str(text.trim())
        .map_err(|_| Error::Answer(format!("anthropic: no JSON answer: {text:.200}")))?;
    let u = &v["usage"];
    let usage = u.is_object().then(|| Usage {
        input_tokens: ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
            .iter()
            .map(|k| u[k].as_u64().unwrap_or(0))
            .sum(),
        output_tokens: u["output_tokens"].as_u64(),
    });
    let answered = v["model"].as_str().unwrap_or(model).to_string();
    prompt::read_output(req, &out, answered, usage)
}

/// Any OpenAI-compatible chat API (Ollama, vLLM, llama.cpp, LM Studio, OpenAI). Probabilities for
/// level questions come from token logprobs when the server returns them.
pub struct OpenAiCompatible {
    pub http: Arc<dyn Http>,
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    /// 0 for one answer per package; the server default when sampling.
    pub temperature: Option<f64>,
    /// Sent as `reasoning_effort` (`none` turns a thinking model's reasoning off: Ollama, OpenAI).
    pub effort: Option<String>,
    pub retry: Retry,
    /// Cleared after the server rejects `logprobs`.
    pub logprobs: AtomicBool,
}

impl OpenAiCompatible {
    fn send(&self, context: &str, req: &Request, logprobs: bool) -> Result<String> {
        let mut body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": prompt::SYSTEM},
                {"role": "user", "content": prompt::render(context, req)},
            ],
            "response_format": {"type": "json_schema", "json_schema": {
                "name": "answers", "strict": true, "schema": prompt::strict_schema(req)}},
        });
        if let Some(t) = self.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(e) = &self.effort {
            body["reasoning_effort"] = json!(e);
        }
        if logprobs {
            body["logprobs"] = json!(true);
            body["top_logprobs"] = json!(5);
        }
        let auth = self.api_key.as_ref().map(|k| format!("Bearer {k}"));
        let mut headers = vec![("content-type", "application/json")];
        if let Some(a) = &auth {
            headers.push(("authorization", a));
        }
        self.retry.post(
            self.http.as_ref(),
            "openai-compatible",
            &format!("{}/chat/completions", base(&self.base_url)),
            &headers,
            &body.to_string(),
        )
    }
}

impl Provider for OpenAiCompatible {
    fn identity(&self) -> String {
        match &self.effort {
            None => format!(
                "openai-compatible:{}:{}:{TEMPLATE_VERSION}",
                base(&self.base_url),
                self.model
            ),
            Some(e) => format!(
                "openai-compatible:{}:{}@{e}:{TEMPLATE_VERSION}",
                base(&self.base_url),
                self.model
            ),
        }
    }

    fn local(&self) -> bool {
        crate::provider::is_loopback(&self.base_url)
    }

    fn classify(&self, context: &str, req: &Request) -> Result<Response> {
        let want = self.logprobs.load(Ordering::SeqCst);
        let text = match self.send(context, req, want) {
            Err(Error::Provider(m)) if want && m.contains("HTTP 400") && m.to_lowercase().contains("logprob") => {
                self.logprobs.store(false, Ordering::SeqCst);
                self.send(context, req, false)?
            }
            other => other?,
        };
        let v: Value = serde_json::from_str(&text).map_err(|e| Error::Answer(format!("openai-compatible: {e}")))?;
        read_openai(req, &v, &self.model)
    }
}

/// Read a chat completion; fill level probabilities from logprobs where they can be found.
pub fn read_openai(req: &Request, v: &Value, model: &str) -> Result<Response> {
    let choice = &v["choices"][0];
    let content = choice["message"]["content"]
        .as_str()
        .ok_or_else(|| Error::Answer(format!("openai-compatible: no message content: {v:.200}")))?;
    let out: Value = serde_json::from_str(content.trim())
        .map_err(|_| Error::Answer(format!("openai-compatible: no JSON answer: {content:.200}")))?;
    let usage = v["usage"].is_object().then(|| Usage {
        input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: v["usage"]["completion_tokens"].as_u64(),
    });
    let mut r = prompt::read_output(req, &out, v["model"].as_str().unwrap_or(model).to_string(), usage)?;
    if let Some(tokens) = choice["logprobs"]["content"].as_array() {
        let mut any = false;
        for (id, q) in req.questions.iter() {
            let Question::Score { criteria, .. } = q else { continue };
            if let Some(p) = level_probabilities(tokens, id, criteria.len()) {
                let top = p.values().cloned().fold(0.0, f64::max);
                let expected = p.iter().map(|(l, x)| l.parse::<f64>().unwrap_or(0.0) * x).sum();
                r.answers.insert(
                    id.clone(),
                    Answer::Score {
                        score: expected,
                        probabilities: p,
                        confidence: Some(top),
                    },
                );
                any = true;
            }
        }
        if any {
            r.confidence_source = ConfidenceSource::Logprobs;
        }
    }
    Ok(r)
}

/// The probabilities of each level at the token holding `"<id>": <level>`, from its top logprobs.
/// The last occurrence counts: a thinking model's logprobs start with its reasoning, which can
/// quote the keys too.
fn level_probabilities(tokens: &[Value], id: &str, levels: usize) -> Option<BTreeMap<String, f64>> {
    let mut text = String::new();
    let mut starts = Vec::with_capacity(tokens.len());
    for t in tokens {
        starts.push(text.len());
        text.push_str(t["token"].as_str()?);
    }
    let key = format!("\"{id}\"");
    let after = text.rfind(&key)? + key.len();
    let rest = &text[after..];
    let colon = rest.find(':')?;
    let digit = after + colon + 1 + rest[colon + 1..].find(|c: char| !c.is_whitespace())?;
    let i = starts.iter().rposition(|s| *s <= digit)?;
    let level_of = |tok: &str| tok.trim().parse::<usize>().ok().filter(|l| *l < levels);
    level_of(tokens[i]["token"].as_str()?)?;
    let mut p: BTreeMap<String, f64> = BTreeMap::new();
    for alt in tokens[i]["top_logprobs"].as_array()? {
        if let (Some(l), Some(lp)) = (alt["token"].as_str().and_then(level_of), alt["logprob"].as_f64()) {
            *p.entry(l.to_string()).or_default() += lp.exp();
        }
    }
    let total: f64 = p.values().sum();
    if total <= 0.0 {
        return None;
    }
    for l in 0..levels {
        let x = p.entry(l.to_string()).or_default();
        *x /= total;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;
    use crate::rubric::QuestionSet;
    use std::time::Duration;

    fn retry() -> Retry {
        Retry {
            attempts: 2,
            base: Duration::ZERO,
        }
    }

    fn answers(set: &QuestionSet) -> Value {
        let mut o = json!({});
        for (id, q) in set.questions.iter() {
            o[id] = match q {
                Question::Noul { .. } => json!(0.9),
                _ => json!(1),
            };
        }
        o
    }

    #[test]
    fn jev_passes_the_request_through() {
        let set = QuestionSet::six_axis();
        let mut ans = json!({});
        for (id, q) in set.questions.iter() {
            ans[id] = match q {
                Question::Noul { .. } => json!({"type": "noul", "noul": 0.8}),
                _ => json!({"type": "score", "score": 1.3, "confidence": 0.5,
                            "legend": {"0": "a"}, "probabilities": {"0": 0.0, "1": 0.7, "2": 0.3, "3": 0.0}}),
            };
        }
        let http = Arc::new(FakeHttp::new(vec![(
            200,
            json!({"model": "jev-1.13.0", "answers": ans, "usage": {"input_tokens": 392, "output_tokens": 65}})
                .to_string(),
        )]));
        let p = Jev {
            http: http.clone(),
            base_url: JEV_URL.into(),
            model: "jev-1.13.0".into(),
            api_key: "k".into(),
            retry: retry(),
        };
        let r = p.classify("ctx", &set.request("state".into())).unwrap();
        assert_eq!(r.confidence_source, ConfidenceSource::Model);
        assert_eq!(r.answers["r2_data_model"].confidence(), Some(0.5));
        let reqs = http.requests.lock().unwrap();
        let (url, headers, body) = &reqs[0];
        assert_eq!(url, "https://api.typesafe.ai/v1/systemone");
        assert!(headers.contains(&("authorization".into(), "Bearer k".into())));
        assert_eq!(body["model"], "jev-1.13.0");
        assert_eq!(body["state"], "state");
        assert_eq!(
            body["questions"]["r2_data_model"]["criteria"].as_array().unwrap().len(),
            4
        );
    }

    #[test]
    fn anthropic_structured_output_and_refusals() {
        let set = QuestionSet::six_axis();
        let req = set.request("s".into());
        let ok = json!({"model": "claude-sonnet-5-5", "stop_reason": "end_turn",
            "content": [{"type": "thinking", "thinking": ""}, {"type": "text", "text": answers(&set).to_string()}],
            "usage": {"input_tokens": 3000, "cache_read_input_tokens": 100, "output_tokens": 80}});
        let http = Arc::new(FakeHttp::new(vec![(200, ok.to_string())]));
        let p = Anthropic {
            http: http.clone(),
            base_url: ANTHROPIC_URL.into(),
            model: "claude-sonnet-5-5".into(),
            api_key: "k".into(),
            effort: Some("low".into()),
            retry: retry(),
        };
        let r = p.classify("ctx", &req).unwrap();
        assert_eq!(r.model, "claude-sonnet-5-5");
        assert_eq!(r.usage.unwrap().input_tokens, 3100);
        assert_eq!(
            p.identity(),
            format!("anthropic:claude-sonnet-5-5:low:{TEMPLATE_VERSION}")
        );
        let reqs = http.requests.lock().unwrap();
        let (url, headers, body) = &reqs[0];
        assert_eq!(url, "https://api.anthropic.com/v1/messages");
        assert!(headers.contains(&("anthropic-version".into(), "2023-06-01".into())));
        assert!(headers.contains(&("anthropic-beta".into(), "server-side-fallback-2026-07-01".into())));
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["output_config"]["effort"], "low");
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        let noul = &body["output_config"]["format"]["schema"]["properties"]["layer_ui"];
        assert!(noul.get("minimum").is_none(), "the API rejects numeric bounds: {noul}");

        let refused = json!({"stop_reason": "refusal", "stop_details": {"category": "cyber"}, "content": []});
        assert!(
            read_anthropic(&req, &refused, "m")
                .unwrap_err()
                .to_string()
                .contains("declined (cyber)")
        );
    }

    #[test]
    fn openai_logprobs_become_level_probabilities() {
        let set = QuestionSet::six_axis();
        let req = set.request("s".into());
        let content = answers(&set).to_string();
        // Tokenize the answer one character at a time; the `r2_data_model` level gets alternatives.
        let at = content.find("\"r2_data_model\":").unwrap() + "\"r2_data_model\":".len();
        let tokens: Vec<Value> = content
            .char_indices()
            .map(|(i, c)| {
                let top = if i == at {
                    json!([{"token": "1", "logprob": (0.6f64).ln()}, {"token": "2", "logprob": (0.3f64).ln()},
                           {"token": "\"", "logprob": (0.1f64).ln()}])
                } else {
                    json!([])
                };
                json!({"token": c.to_string(), "logprob": 0.0, "top_logprobs": top})
            })
            .collect();
        let v = json!({"model": "qwen3-coder:30b", "choices": [{"message": {"content": content},
            "logprobs": {"content": tokens}}], "usage": {"prompt_tokens": 5000, "completion_tokens": 90}});
        let r = read_openai(&req, &v, "m").unwrap();
        assert_eq!(r.confidence_source, ConfidenceSource::Logprobs);
        match &r.answers["r2_data_model"] {
            Answer::Score {
                probabilities,
                confidence,
                ..
            } => {
                assert!((probabilities["1"] - 2.0 / 3.0).abs() < 1e-9);
                assert!((probabilities["2"] - 1.0 / 3.0).abs() < 1e-9);
                assert_eq!(probabilities["0"], 0.0);
                assert!((confidence.unwrap() - 2.0 / 3.0).abs() < 1e-9);
            }
            a => panic!("{a:?}"),
        }
        // Reasoning that quotes the key first (a thinking model's logprobs include it) is skipped.
        let mut thinking: Vec<Value> = "I think \"r2_data_model\": 3 maybe. "
            .chars()
            .map(|c| json!({"token": c.to_string(), "logprob": 0.0, "top_logprobs": [{"token": "3", "logprob": 0.0}]}))
            .collect();
        thinking.extend(tokens.iter().cloned());
        let v2 = json!({"choices": [{"message": {"content": content}, "logprobs": {"content": thinking}}]});
        match &read_openai(&req, &v2, "m").unwrap().answers["r2_data_model"] {
            Answer::Score { probabilities, .. } => assert!((probabilities["1"] - 2.0 / 3.0).abs() < 1e-9),
            a => panic!("{a:?}"),
        }
        // Without logprobs: the plain answer.
        let v = json!({"choices": [{"message": {"content": content}}]});
        assert_eq!(
            read_openai(&req, &v, "m").unwrap().confidence_source,
            ConfidenceSource::None
        );
    }

    #[test]
    fn openai_retries_without_logprobs_when_rejected() {
        let set = QuestionSet::six_axis();
        let ok = json!({"choices": [{"message": {"content": answers(&set).to_string()}}]});
        let http = Arc::new(FakeHttp::new(vec![
            (400, "logprobs are not supported".into()),
            (200, ok.to_string()),
        ]));
        let p = OpenAiCompatible {
            http: http.clone(),
            base_url: "http://localhost:11434/v1/".into(),
            model: "qwen3-coder:30b".into(),
            api_key: None,
            temperature: Some(0.0),
            effort: Some("none".into()),
            retry: retry(),
            logprobs: AtomicBool::new(true),
        };
        assert!(p.local());
        p.classify("ctx", &set.request("s".into())).unwrap();
        let reqs = http.requests.lock().unwrap();
        assert_eq!(reqs[0].0, "http://localhost:11434/v1/chat/completions");
        assert_eq!(reqs[0].2["logprobs"], true);
        assert!(reqs[1].2.get("logprobs").is_none());
        assert_eq!(reqs[1].2["temperature"], 0.0);
        assert_eq!(reqs[1].2["response_format"]["json_schema"]["strict"], true);
        assert_eq!(reqs[1].2["reasoning_effort"], "none");
        assert!(p.identity().contains("qwen3-coder:30b@none:"));
    }
}
