# pmx-sast@1 — bundled own-code SAST rules

The semgrep rules pmx runs by default for the own-code SAST trend (wiki/metrics-spec.md §7.4).
Written for pmx from scratch, **MIT OR Apache-2.0** like the rest of the repository. They are not
derived from the Semgrep registry or the Opengrep rules, whose licences don't allow bundling here.

## Scope

A small, precise set: patterns that are almost always a real risk (injection, unsafe
deserialization, disabled TLS checks, public storage) in the stacks pmx sees. SAST is a **trend**
signal (never ranked across projects), so a rule must stay quiet on ordinary code: database
migrations are excluded from the SQL rules, and every rule has "must not flag" tests. Projects can
add their own rules next to these (`[snapshots] sast_rules = ["bundled", "path/to/rules"]`).

## Rules

pmx buckets severities as ERROR → high, WARNING → medium, INFO → low.

| Rule | Languages | pmx severity | CWE | Flags |
|---|---|---|---|---|
| `pmx-cs-sql-injection` | csharp | high | CWE-89 | SQL is built by concatenation or interpolation |
| `pmx-cs-binaryformatter` | csharp | high | CWE-502 | BinaryFormatter (and NetDataContractSerializer, LosFormatter) deserialization can run code and is obsolete |
| `pmx-cs-tls-validation-disabled` | csharp | high | CWE-295 | A certificate validation callback that always returns true accepts any server |
| `pmx-go-sql-injection` | go | high | CWE-89 | SQL is built with Sprintf or concatenation |
| `pmx-go-tls-insecure-skip-verify` | go | high | CWE-295 | InsecureSkipVerify turns off certificate checks, so any server can impersonate the real one |
| `pmx-go-command-injection` | go | high | CWE-78 | A shell runs a command built from a non-literal |
| `pmx-go-weak-hash` | go | low | CWE-328 | MD5 and SHA-1 are broken for security purposes |
| `pmx-java-sql-injection` | java | high | CWE-89 | SQL (or JPQL) is built by concatenation |
| `pmx-java-command-injection` | java | high | CWE-78 | A command is run from a non-literal string |
| `pmx-java-unsafe-deserialization` | java | medium | CWE-502 | Java deserialization of untrusted bytes can run code (gadget chains) |
| `pmx-java-tls-trust-all` | java | high | CWE-295 | A HostnameVerifier that accepts every host turns off TLS identity checks |
| `pmx-js-eval` | javascript, typescript | high | CWE-95 | Code built from a non-literal is evaluated |
| `pmx-js-command-injection` | javascript, typescript | high | CWE-78 | A shell command is built from a non-literal |
| `pmx-js-sql-injection` | javascript, typescript | high | CWE-89 | SQL is built by concatenating or interpolating values |
| `pmx-js-prisma-raw-unsafe` | javascript, typescript | high | CWE-89 | $queryRawUnsafe / $executeRawUnsafe run the string as SQL |
| `pmx-js-dom-xss` | javascript, typescript | medium | CWE-79 | HTML is written from a non-literal |
| `pmx-js-tls-verification-disabled` | javascript, typescript | high | CWE-295 | TLS certificate checks are turned off, so any server can impersonate the real one |
| `pmx-js-open-redirect` | javascript, typescript | medium | CWE-601 | The redirect target comes straight from the request, so a link to this site can send users to any site |
| `pmx-js-weak-hash` | javascript, typescript | low | CWE-328 | MD5 and SHA-1 are broken for security purposes |
| `pmx-js-jwt-none-algorithm` | javascript, typescript | high | CWE-347 | Accepting the "none" algorithm means unsigned tokens pass verification |
| `pmx-kotlin-sql-injection` | kotlin | high | CWE-89 | SQL is built from a string template or concatenation |
| `pmx-kotlin-command-injection` | kotlin | high | CWE-78 | A command is run from a non-literal string |
| `pmx-android-webview-js-bridge` | kotlin, java | medium | CWE-749 | addJavascriptInterface exposes app methods to every page the WebView loads |
| `pmx-android-world-readable` | kotlin, java | medium | CWE-276 | MODE_WORLD_READABLE / MODE_WORLD_WRITEABLE let other apps read or change this data |
| `pmx-py-eval` | python | high | CWE-95 | Code built from a non-literal is evaluated |
| `pmx-py-command-injection` | python | high | CWE-78 | A shell runs a command built from a non-literal |
| `pmx-py-sql-injection` | python | high | CWE-89 | SQL is built with string formatting |
| `pmx-py-unsafe-deserialization` | python | medium | CWE-502 | This deserializer can run code from the data it reads |
| `pmx-py-tls-verification-disabled` | python | high | CWE-295 | verify=False turns off certificate checks, so any server can impersonate the real one |
| `pmx-py-debug-enabled` | python | medium | CWE-489 | The debug server/mode exposes an interactive console and stack traces |
| `pmx-react-dangerous-html` | javascript, typescript | medium | CWE-79 | dangerouslySetInnerHTML renders a non-literal as HTML |
| `pmx-react-javascript-url` | javascript, typescript | medium | CWE-79 | A link target from a variable can be a javascript: URL, which runs code when clicked |
| `pmx-tf-public-bucket-acl` | hcl | high | CWE-732 | This bucket ACL makes objects public |
| `pmx-tf-admin-port-open-to-world` | hcl | high | CWE-284 | SSH/RDP is open to the whole internet |

## Changing the rules

- **Any change is a new version** (`pmx-sast@2`, a new folder): the SAST trend compares counts over
  time, and a rule change would look like a change in the code. Results are cached by the rules'
  content, so a new version rescans. `meta.tool_versions.sast_rules` records which set ran.
- Every rule needs tests in the file with the same name (`go.yaml` → `go.go`): a `ruleid:` comment
  above each line that must be flagged and an `ok:` comment above lines that must not.
- `python3 check.py` runs semgrep's tests and checks that no finding is reported twice (semgrep's
  tests can't see that). CI runs it with the pinned semgrep.
- Rule ids start with `pmx-` and contain no dots. In `metavariable-regex`, use non-capturing groups
  `(?:…)` (capture groups become `$1`… metavariables) and remember that the regex is anchored at the
  start of the matched text.
- Add the file to `RULES` in `crates/pm-snapshot/src/bundled.rs` (a test checks the list).
