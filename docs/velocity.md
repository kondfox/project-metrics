# Velocity: setting up a classifier

Velocity is the amount of hard work delivered per unit of capacity. pmx groups commits into **work
packages** (a ticket, else the merged pull request, else one author's week). It sends each package's diff
to an LLM, the **classifier**, which answers a fixed set of complexity questions. The answers become
points. Velocity is points ÷ FTE-days (from `[fte]`), or ÷ active engineer-days when the FTE is unknown.
The full definition is in [metrics-spec.md §4](../wiki/metrics-spec.md) and
[velocity-index.md](../wiki/velocity-index.md).

Velocity is optional. Without a classifier it shows as "not measured" and every other metric works.

## Before you start: where the code goes

A classifier reads your **diffs**. With a remote provider (Claude Code, the Anthropic API, Jev, OpenAI),
code leaves your machine. Check that your client contracts allow this. For repos that must not leave the
machine, add `data_policy = "local-only"` to that `[[repo]]`. pmx then refuses any classifier
that is not on this machine, and `pmx check` reports it before anything is sent.

## Pick a provider

| You have | Provider | Code leaves the machine |
|---|---|---|
| A Claude subscription and the [Claude Code CLI](https://docs.claude.com/en/docs/claude-code) | `claude-code` | yes (to Anthropic) |
| An Anthropic API key | `anthropic` | yes (to Anthropic) |
| A local model in Ollama, LM Studio, vLLM or llama.cpp | `openai-compatible` | no (loopback) |
| An OpenAI-compatible API | `openai-compatible` | yes |
| Jev access | `jev` | yes (to Jev) |
| Your own program | `command` | depends on the program |

`pmx init` offers `claude-code` when it finds the `claude` CLI, and Ollama when it is running. To add one
later, edit `pmx.toml`:

```toml
[classifiers.claude]
provider = "claude-code"
model = "claude-sonnet-5-5"     # pin a versioned id: answers are cached per model

[velocity]
classifier = "claude"
since = "2026-07-01"            # score packages from this date (scoring is the slow step)
```

More examples (Anthropic API, Jev, a command, per-repo overrides, the confidence gate): the
`[classifiers.*]` sections of [examples/pmx.toml](../examples/pmx.toml).

### A local model with Ollama

1. Install the **Ollama app** (`brew install --cask ollama-app` on macOS). The Homebrew formula
   `ollama` lacks the library for structured output, and every request then fails with "structured output
   is unavailable".
2. Start it with a larger context: `OLLAMA_CONTEXT_LENGTH=16384 ollama serve`. pmx's prompts reach ~10k
   tokens, more than Ollama's default.
3. Pull a model, e.g. `ollama pull qwen3.5:9b` (~7 GB; fits an 18 GB laptop).
4. Configure it with reasoning off. With reasoning on, a single answer can take minutes.

```toml
[classifiers.local]
provider = "openai-compatible"
base_url = "http://localhost:11434/v1"
model = "qwen3.5:9b"
effort = "none"
```

## Check and run

```sh
pmx classifier test claude   # scores one small fictional package: is it installed and logged in?
pmx check                    # the data policy, keys and classifier names
pmx collect                  # scores new packages; answers are cached, so re-runs don't pay again
```

During `collect`, the Velocity stage shows how many packages are left. A package that fails stays
**unscored**. Any week or month containing one has no Velocity (never a partial sum). Re-run
`collect` to retry. `pmx collect --no-velocity` skips scoring for one run.

## How fast, how much

Each package is one request of a few thousand tokens. Expect roughly 5–30 s per package, with `jobs`
packages in flight (default 4). A busy team produces 50–150 packages a month. Use `[velocity] since` to
score only recent months on the first run, then move it back if you want more history.

## Comparing classifiers

Scores from different models are not strictly comparable. The fleet view pools Velocity only across
projects scored the same way. To qualify a new classifier against reference scores, use
`pmx classifier eval` ([plan §6.5](../wiki/tool-implementation-plan.md)).
