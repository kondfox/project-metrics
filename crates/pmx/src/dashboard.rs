//! The project dashboard (plan M2): one self-contained HTML page with the data, ECharts and the
//! pm-metrics WASM engine inlined, so it works from `pmx serve`, from a shared file, or opened
//! straight from disk.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

use anyhow::{Context, Result};
use pm_metrics::ProjectFile;

const TEMPLATE: &str = include_str!("../web/index.html");
const CSS: &str = include_str!("../web/style.css");
const APP: &str = include_str!("../web/app.js");
const ECHARTS: &str = include_str!("../web/vendor/echarts.min.js");
const WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/pm_wasm.wasm"));

/// Is the metrics engine built in (see `build.rs`)?
pub fn has_engine() -> bool {
    !WASM.is_empty()
}

/// The embedded pm-metrics WASM module (empty without the wasm32 target).
pub fn engine_wasm() -> &'static [u8] {
    WASM
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// JSON is safe inside `<script>` once `</` can't end the element early.
fn script_safe(json: &str) -> String {
    json.replace("</", "<\\/").replace("<!--", "<\\!--")
}

/// Fill `{{KEY}}` placeholders in one pass, so inserted text is never scanned again.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len() + values.iter().map(|(_, v)| v.len()).sum::<usize>());
    let mut rest = template;
    while let Some(i) = rest.find("{{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        match after
            .find("}}")
            .and_then(|j| values.iter().find(|(k, _)| *k == &after[..j]).map(|(_, v)| (j, v)))
        {
            Some((j, v)) => {
                out.push_str(v);
                rest = &after[j + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The page for a `project.json` (and, for the lead's own view, `private/leads.json`).
pub fn render(project_json: &str, leads_json: Option<&str>) -> Result<String> {
    let p: ProjectFile = serde_json::from_str(project_json).context("project.json is not a pmx project file")?;
    if let Some(l) = leads_json {
        serde_json::from_str::<serde_json::Value>(l).context("leads.json is not JSON")?;
    }
    let data = format!(
        "{{\"project\":{},\"leads\":{},\"wasm\":\"{}\"}}",
        script_safe(project_json),
        leads_json.map(script_safe).unwrap_or_else(|| "null".into()),
        base64(WASM)
    );
    let title = html_escape(&format!("{} · pmx", p.project.name));
    Ok(fill(
        TEMPLATE,
        &[
            ("TITLE", &title),
            ("CSS", CSS),
            ("DATA", &data),
            ("ECHARTS", ECHARTS),
            ("APP", APP),
        ],
    ))
}

/// Render from a workspace's `out/` folder.
pub fn render_workspace(out_dir: &Path, with_private: bool) -> Result<String> {
    let path = out_dir.join("project.json");
    let project = std::fs::read_to_string(&path)
        .with_context(|| format!("reading {} (run `pmx collect` first)", path.display()))?;
    let leads = if with_private {
        std::fs::read_to_string(out_dir.join("private").join("leads.json")).ok()
    } else {
        None
    };
    render(&project, leads.as_deref())
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

fn handle(mut stream: TcpStream, out_dir: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }
    let path = request.split_whitespace().nth(1).unwrap_or("/");
    match path.split('?').next().unwrap_or("/") {
        "/" | "/index.html" => match render_workspace(out_dir, true) {
            Ok(html) => respond(&mut stream, "200 OK", "text/html; charset=utf-8", html.as_bytes()),
            Err(e) => respond(
                &mut stream,
                "500 Internal Server Error",
                "text/plain; charset=utf-8",
                format!("{e:#}").as_bytes(),
            ),
        },
        _ => respond(&mut stream, "404 Not Found", "text/plain", b"not found"),
    }
}

/// Serve the dashboard on a loopback listener until the process ends. Each request re-reads
/// `out/`, so a new `pmx collect` shows up on reload. The private lead view is included: the
/// listener only accepts local connections.
pub fn serve(listener: TcpListener, out_dir: &Path) -> Result<()> {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                if let Err(e) = handle(s, out_dir) {
                    eprintln!("request failed: {e}");
                }
            }
            Err(e) => eprintln!("connection failed: {e}"),
        }
    }
    Ok(())
}

/// Open a URL in the default browser.
pub fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        ("open", vec![url])
    } else if cfg!(target_os = "windows") {
        ("cmd", vec!["/C", "start", "", url])
    } else {
        ("xdg-open", vec![url])
    };
    if let Err(e) = std::process::Command::new(cmd.0).args(cmd.1).spawn() {
        eprintln!("cannot open a browser ({e}); open {url}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn fill_is_single_pass() {
        let out = fill("a{{X}}b{{Y}}c{{Z}}", &[("X", "{{Y}}"), ("Y", "y")]);
        assert_eq!(out, "a{{Y}}byc{{Z}}");
    }

    #[test]
    fn inlined_assets_cannot_close_their_elements() {
        for (name, text, closer) in [
            ("echarts", ECHARTS, "</script"),
            ("app.js", APP, "</script"),
            ("style.css", CSS, "</style"),
        ] {
            let lower = text.to_lowercase();
            assert!(
                !lower.contains(closer) && !lower.contains("<!--"),
                "{name} must not contain {closer} or <!--"
            );
        }
    }

    #[test]
    fn script_safety() {
        assert_eq!(script_safe(r#"{"name":"</script><b>"}"#), r#"{"name":"<\/script><b>"}"#);
        let v: serde_json::Value = serde_json::from_str(&script_safe(r#"{"a":"</x>"}"#)).unwrap();
        assert_eq!(v["a"], "</x>");
    }
}
