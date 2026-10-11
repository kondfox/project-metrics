//! Providers fill the shared schema (plan §6.2): `claude-code` (the Claude Code CLI under the user's
//! own login, D28) and `command` (any local program) here; the HTTP APIs in [`crate::api`].

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pm_config::{ClassifierConfig, Provider as Kind};
use serde_json::{Value, json};

use crate::api::{ANTHROPIC_URL, Anthropic, JEV_URL, Jev, OpenAiCompatible};
use crate::http::{Http, Retry, UreqHttp};
use crate::prompt::{self, Template};
use crate::schema::{Request, Response, Usage};
use crate::strategy::{Isolated, Sampled};
use crate::{Error, Result};

pub const DEFAULT_TIMEOUT_SECS: u64 = 300;
pub const DEFAULT_CLAUDE_MODEL: &str = "sonnet";

pub trait Provider: Send + Sync {
    /// `provider:model:template`: part of the answer cache key, and recorded per package.
    fn identity(&self) -> String;
    /// Does the state stay on this machine? (`data_policy = "local-only"`.)
    fn local(&self) -> bool;
    /// Answer every question about `req.state`. `context` is the rubric's shared instructions.
    fn classify(&self, context: &str, req: &Request) -> Result<Response>;
}

/// Does this classifier keep code on the machine? A `command` is trusted to be local; an
/// OpenAI-compatible server is local when its URL is loopback; every hosted API (and the Claude
/// Code CLI) is not.
pub fn is_local(c: &ClassifierConfig) -> bool {
    match c.provider {
        Kind::Command => true,
        Kind::OpenaiCompatible => c.base_url.as_deref().is_some_and(is_loopback),
        Kind::Jev | Kind::Anthropic | Kind::ClaudeCode => false,
    }
}

/// Is `url`'s host loopback (`localhost`, `127.*`, `::1`)?
pub fn is_loopback(url: &str) -> bool {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or("");
    let authority = authority.rsplit_once('@').map(|(_, h)| h).unwrap_or(authority);
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost") || host.starts_with("127.") || host == "::1"
}

/// The provider for one `[classifiers.<name>]`, over real HTTP. Generative providers render the
/// questions with `template` (the question set's).
pub fn build(c: &ClassifierConfig, template: Template) -> Result<Box<dyn Provider>> {
    let timeout = Duration::from_secs(c.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    build_with(c, Arc::new(UreqHttp::new(timeout)), template)
}

/// [`build`] over another transport (tests).
pub fn build_with(c: &ClassifierConfig, http: Arc<dyn Http>, template: Template) -> Result<Box<dyn Provider>> {
    let timeout = Duration::from_secs(c.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let model = || {
        c.model
            .clone()
            .ok_or_else(|| Error::Config(format!("provider = \"{}\" needs `model`", c.provider.as_str())))
    };
    let key = |default_env: &str| -> Result<String> {
        let env = c.api_key_env.as_deref().unwrap_or(default_env);
        std::env::var(env)
            .ok()
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| Error::Config(format!("${env} is not set")))
    };
    let retry = Retry::default();
    let samples = c.samples.unwrap_or(1).max(1);
    let base: Box<dyn Provider> = match c.provider {
        Kind::ClaudeCode => Box::new(ClaudeCode {
            program: c.command.clone().unwrap_or_else(|| vec!["claude".into()]),
            model: c.model.clone().unwrap_or_else(|| DEFAULT_CLAUDE_MODEL.into()),
            effort: c.effort.clone(),
            timeout,
            template,
        }),
        Kind::Command => Box::new(CommandProvider {
            program: c
                .command
                .clone()
                .filter(|v| !v.is_empty())
                .ok_or_else(|| Error::Config("provider = \"command\" needs `command`".into()))?,
            model: c.model.clone(),
            timeout,
        }),
        Kind::Jev => Box::new(Jev {
            http,
            base_url: c.base_url.clone().unwrap_or_else(|| JEV_URL.into()),
            model: model()?,
            api_key: key("TYPESAFE_API_KEY")?,
            retry,
        }),
        Kind::Anthropic => Box::new(Anthropic {
            http,
            base_url: c.base_url.clone().unwrap_or_else(|| ANTHROPIC_URL.into()),
            model: model()?,
            api_key: key("ANTHROPIC_API_KEY")?,
            effort: c.effort.clone(),
            retry,
            template,
        }),
        Kind::OpenaiCompatible => Box::new(OpenAiCompatible {
            http,
            base_url: c
                .base_url
                .clone()
                .ok_or_else(|| Error::Config("provider = \"openai-compatible\" needs `base_url`".into()))?,
            model: model()?,
            api_key: match &c.api_key_env {
                Some(_) => Some(key("")?),
                None => None,
            },
            // One deterministic answer, unless sampling.
            temperature: (samples == 1).then_some(0.0),
            effort: c.effort.clone(),
            retry,
            logprobs: AtomicBool::new(true),
            template,
        }),
    };
    // Jev already judges each question on its own and returns probabilities.
    let base: Box<dyn Provider> = if c.isolate_questions == Some(true) && c.provider != Kind::Jev {
        Box::new(Isolated { inner: base })
    } else {
        base
    };
    Ok(if samples > 1 && c.provider != Kind::Jev {
        Box::new(Sampled {
            inner: base,
            k: samples,
        })
    } else {
        base
    })
}

/// `claude -p` with a forced JSON schema, no tools, no session and no settings, from an empty
/// directory (so no project instructions are read). Authenticates however Claude Code is logged in
/// on this machine: a Claude subscription, or an API key if one is set.
pub struct ClaudeCode {
    pub program: Vec<String>,
    pub model: String,
    pub effort: Option<String>,
    pub timeout: Duration,
    pub template: Template,
}

impl Provider for ClaudeCode {
    fn identity(&self) -> String {
        let t = self.template.version();
        match &self.effort {
            None => format!("claude-code:{}:{t}", self.model),
            Some(e) => format!("claude-code:{}@{e}:{t}", self.model),
        }
    }

    fn local(&self) -> bool {
        false
    }

    fn classify(&self, context: &str, req: &Request) -> Result<Response> {
        let schema = self.template.output_schema(req).to_string();
        let dir = tempfile::tempdir().map_err(Error::Spawn)?;
        let mut cmd = Command::new(&self.program[0]);
        cmd.args(&self.program[1..])
            .args(["-p", "--model", &self.model, "--output-format", "json"])
            .args(["--json-schema", &schema])
            .args(["--system-prompt", prompt::SYSTEM])
            .args(["--tools", ""])
            .args(["--setting-sources", ""])
            .args([
                "--strict-mcp-config",
                "--disable-slash-commands",
                "--no-session-persistence",
            ])
            .current_dir(dir.path());
        if let Some(e) = &self.effort {
            cmd.args(["--effort", e]);
        }
        let out = run(cmd, self.template.render(context, req).as_bytes(), self.timeout)?;
        let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| {
            Error::Provider(format!(
                "claude exited {} without JSON: {}",
                out.status,
                tail(&out.stdout, &out.stderr)
            ))
        })?;
        read_claude(self.template, req, &v, &self.model)
    }
}

/// Read `claude -p --output-format json` output.
pub fn read_claude(template: Template, req: &Request, v: &Value, model: &str) -> Result<Response> {
    if v["is_error"].as_bool() == Some(true) || v["subtype"].as_str().is_some_and(|s| s != "success") {
        let msg = v["result"].as_str().unwrap_or("unknown error");
        return Err(Error::Provider(format!("claude: {msg}")));
    }
    let answer = match &v["structured_output"] {
        o @ Value::Object(_) => o.clone(),
        _ => {
            // Older CLIs put the JSON in `result`, sometimes fenced.
            let text = v["result"].as_str().unwrap_or("");
            let text = text.trim().trim_start_matches("```json").trim_matches('`').trim();
            serde_json::from_str(text)
                .map_err(|_| Error::Answer(format!("claude returned no structured output: {text:.200}")))?
        }
    };
    // `modelUsage` names the model that actually answered (the alias resolves there).
    let used = v["modelUsage"].as_object().and_then(|m| {
        m.iter()
            .max_by_key(|(_, u)| u["outputTokens"].as_u64().unwrap_or(0))
            .map(|(k, _)| k.clone())
    });
    let u = &v["usage"];
    let usage = u.is_object().then(|| Usage {
        input_tokens: ["input_tokens", "cache_read_input_tokens", "cache_creation_input_tokens"]
            .iter()
            .map(|k| u[k].as_u64().unwrap_or(0))
            .sum(),
        output_tokens: u["output_tokens"].as_u64(),
    });
    template.read_output(req, &answer, used.unwrap_or_else(|| model.to_string()), usage)
}

/// Any program: `{context, state, questions}` JSON on stdin, a [`Response`] on stdout.
pub struct CommandProvider {
    pub program: Vec<String>,
    pub model: Option<String>,
    pub timeout: Duration,
}

impl Provider for CommandProvider {
    fn identity(&self) -> String {
        format!(
            "command:{}:{}",
            self.program.join(" "),
            self.model.as_deref().unwrap_or("-")
        )
    }

    fn local(&self) -> bool {
        true
    }

    fn classify(&self, context: &str, req: &Request) -> Result<Response> {
        let input = json!({"context": context, "state": req.state, "questions": req.questions});
        let mut cmd = Command::new(&self.program[0]);
        cmd.args(&self.program[1..]);
        let out = run(cmd, input.to_string().as_bytes(), self.timeout)?;
        if !out.status.success() {
            return Err(Error::Provider(format!(
                "{} exited {}: {}",
                self.program[0],
                out.status,
                tail(&out.stdout, &out.stderr)
            )));
        }
        let r: Response = serde_json::from_slice(&out.stdout)
            .map_err(|e| Error::Answer(format!("{}: not a response ({e})", self.program[0])))?;
        for (id, _) in req.questions.iter() {
            if !r.answers.contains_key(id) {
                return Err(Error::Answer(format!("no answer to `{id}`")));
            }
        }
        Ok(r)
    }
}

/// The last bit of output, for error messages.
fn tail(stdout: &[u8], stderr: &[u8]) -> String {
    let s = if stderr.iter().any(|b| !b.is_ascii_whitespace()) {
        stderr
    } else {
        stdout
    };
    let s = String::from_utf8_lossy(s);
    let s = s.trim();
    let start = s.char_indices().rev().nth(300).map_or(0, |(i, _)| i);
    s[start..].to_string()
}

/// Run with `input` on stdin; kill it after `timeout`.
fn run(mut cmd: Command, input: &[u8], timeout: Duration) -> Result<std::process::Output> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(Error::Spawn)?;
    let mut stdin = child.stdin.take().expect("piped");
    let input = input.to_vec();
    let writer = std::thread::spawn(move || {
        // A child that exits early closes the pipe; that error shows up as its exit status.
        let _ = stdin.write_all(&input);
    });
    let mut stdout = child.stdout.take().expect("piped");
    let mut stderr = child.stderr.take().expect("piped");
    let out_reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err_reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        b
    });
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(Error::Spawn)? {
            break s;
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Timeout(timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = writer.join();
    Ok(std::process::Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rubric::QuestionSet;

    #[test]
    fn locality() {
        let c = |p: Kind, url: Option<&str>| ClassifierConfig {
            provider: p,
            base_url: url.map(String::from),
            ..Default::default()
        };
        assert!(is_local(&c(Kind::Command, None)));
        assert!(is_local(&c(Kind::OpenaiCompatible, Some("http://localhost:11434/v1"))));
        assert!(is_local(&c(Kind::OpenaiCompatible, Some("http://127.0.0.1:8000"))));
        assert!(is_local(&c(Kind::OpenaiCompatible, Some("http://[::1]:8000/v1"))));
        assert!(!is_local(&c(
            Kind::OpenaiCompatible,
            Some("https://api.example.com/v1")
        )));
        assert!(!is_local(&c(
            Kind::OpenaiCompatible,
            Some("http://localhost.example.com")
        )));
        assert!(!is_local(&c(Kind::ClaudeCode, None)));
        assert!(!is_local(&c(Kind::Jev, None)));
    }

    fn answers(set: &QuestionSet) -> Value {
        let mut o = json!({});
        for (id, q) in set.questions.iter() {
            o[id] = match q {
                crate::Question::Noul { .. } => json!(0.25),
                _ => json!(2),
            };
        }
        o
    }

    #[test]
    fn reads_claude_output() {
        let set = QuestionSet::v2();
        let req = set.request("s".into(), &[]);
        let v = json!({
            "type": "result", "subtype": "success", "is_error": false,
            "result": "", "structured_output": answers(&set),
            "usage": {"input_tokens": 10, "cache_read_input_tokens": 1000, "output_tokens": 50},
            "modelUsage": {"claude-haiku-5-5": {"outputTokens": 3}, "claude-sonnet-5-5": {"outputTokens": 50}},
        });
        let r = read_claude(Template::V1, &req, &v, "sonnet").unwrap();
        assert_eq!(r.model, "claude-sonnet-5-5");
        assert_eq!(r.usage.unwrap().input_tokens, 1010);
        assert_eq!(r.answers.len(), 13);

        // JSON in `result` (fenced) works too.
        let v = json!({"subtype": "success", "result": format!("```json\n{}\n```", answers(&set))});
        assert_eq!(read_claude(Template::V1, &req, &v, "sonnet").unwrap().model, "sonnet");

        let v = json!({"subtype": "success", "is_error": true, "result": "Failed to authenticate"});
        assert!(
            read_claude(Template::V1, &req, &v, "sonnet")
                .unwrap_err()
                .to_string()
                .contains("Failed to authenticate")
        );
    }

    #[test]
    fn reads_stated_probabilities_under_v3() {
        let set = QuestionSet::v3();
        let files = [crate::LayerFile::new("shop-web", "src/Profile.tsx", true, 46)];
        let req = set.request("s".into(), &files);
        let mut o = json!({});
        for (id, q) in req.questions.iter() {
            o[id] = match q {
                crate::Question::Noul { .. } => json!(0.3),
                _ => json!({"option": "ui", "p": 0.85}),
            };
        }
        let v = json!({"subtype": "success", "is_error": false, "structured_output": o});
        let r = read_claude(set.template, &req, &v, "claude-opus-5-5").unwrap();
        assert_eq!(r.confidence_source, crate::ConfidenceSource::Stated);
        assert_eq!(r.answers["file_1"].confidence(), Some(0.85));
        let p = ClaudeCode {
            program: vec!["claude".into()],
            model: "claude-opus-5-5".into(),
            effort: None,
            timeout: Duration::from_secs(1),
            template: set.template,
        };
        assert_eq!(p.identity(), "claude-code:claude-opus-5-5:pmx-prompt@2");
    }

    #[cfg(unix)]
    #[test]
    fn runs_programs_with_a_timeout() {
        let set = QuestionSet::v2();
        let req = set.request("s".into(), &[]);
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("fake-claude");
        let out = json!({"subtype": "success", "is_error": false, "structured_output": answers(&set),
                         "modelUsage": {"claude-sonnet-5-5": {"outputTokens": 9}}});
        // Records its arguments and stdin, then answers like `claude -p`.
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"{d}/args\"\ncat > \"{d}/stdin\"\npwd > \"{d}/cwd\"\ncat <<'EOF'\n{out}\nEOF\n",
            d = dir.path().display()
        );
        std::fs::write(&fake, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let p = ClaudeCode {
            program: vec![fake.display().to_string()],
            model: "claude-sonnet-5-5".into(),
            effort: Some("low".into()),
            timeout: Duration::from_secs(20),
            template: Template::V1,
        };
        let r = p.classify(&set.context, &req).unwrap();
        assert_eq!(
            r.answers["r4_algorithmic"],
            crate::Answer::Score {
                score: 2.0,
                probabilities: [("2".to_string(), 1.0)].into(),
                confidence: None,
            }
        );
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert!(args.contains("--json-schema\n{"));
        assert!(args.contains("--tools\n\n"), "no tools: {args}");
        assert!(args.contains("--effort\nlow\n"));
        assert!(
            std::fs::read_to_string(dir.path().join("stdin"))
                .unwrap()
                .contains("<state>\ns\n</state>")
        );
        let cwd = std::fs::read_to_string(dir.path().join("cwd")).unwrap();
        assert!(
            !cwd.trim().is_empty() && !std::path::Path::new(cwd.trim()).exists(),
            "ran in a temp dir"
        );

        let slow = CommandProvider {
            program: vec!["sh".into(), "-c".into(), "sleep 5".into()],
            model: None,
            timeout: Duration::from_millis(200),
        };
        assert!(matches!(slow.classify("", &req), Err(Error::Timeout(_))));
        let failing = CommandProvider {
            program: vec!["sh".into(), "-c".into(), "echo boom >&2; exit 3".into()],
            model: None,
            timeout: Duration::from_secs(20),
        };
        assert!(failing.classify("", &req).unwrap_err().to_string().contains("boom"));
    }
}
