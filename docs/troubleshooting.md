# Troubleshooting

**Which tools are missing?** `pmx doctor` lists git and the snapshot tools, says what each one measures,
and says how to install it. Without a tool, its metrics show as "not measured", never as 0.

| Symptom | Cause and fix |
|---|---|
| `pmx init` finds no repos | It looks two folder levels deep. Pass the folder that holds the clones: `pmx init ~/src/acme`, or add one with `pmx repo add <path or URL>` |
| A repo has the wrong stack | `pmx repo set <name> --role frontend` (or `per-file` for a repo that mixes stacks) |
| One person shows up twice | `pmx people merge "Jane Doe" jane@home.example` |
| A contractor or bot is counted | `pmx people external "<name>"` or `pmx people bot <email or login>` |
| `pmx check`: identity not in `[people]` | Run `pmx people` and answer yes, or merge it by hand as above |
| `pmx check`: token is not set | Export the variable named in `[code_host] token_env` (default `GITHUB_TOKEN` / `GITLAB_TOKEN`) with read access to pull requests. Without it the PR metrics are "not measured" |
| `pmx check`: branch does not resolve | `pmx repo set <name> --branch origin/main`. `pmx collect` fetches first; `--no-fetch` skips that |
| Peer review is "not measured" | No token, or the repo has no `code_host_id` (`pmx repo set <name> --code-host-id owner/repo`) |
| Duplication or SAST is "not measured" | jscpd needs Node (it runs through `npx`); SAST needs semgrep (`pipx install semgrep==1.179.0`) |
| Velocity is "not measured" | No classifier, or some packages in that period failed to score: see [velocity.md](velocity.md) |
| Velocity: "structured output is unavailable" | Ollama from the Homebrew formula: install the Ollama app instead ([velocity.md](velocity.md)) |
| `collect` was interrupted | Run it again: finished work is cached and it continues |
| Numbers look off after editing `pmx.toml` | Changing `[people]`, roles or `range_start` redoes only the affected stages on the next collect. `pmx collect --full` recomputes everything |
| Port 7878 is in use | `pmx serve --port 7979 --open` |
| The dashboard says "Dashboard not built" | This pmx was built from source without Node. Install a release binary, or Node, then rebuild |

Still stuck? Open an issue with the command, what you expected, and the output. Remove any real names,
emails, repo URLs and metric values first.
