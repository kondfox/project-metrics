# Getting started

From download to your project's dashboard in about 15 minutes. You need **git ≥ 2.30** and the git
clones of your project's repositories. Everything stays on your machine.

## 1. Install (1 minute)

| Platform | Command |
|---|---|
| macOS, Linux (Homebrew) | `brew install kondfox/tap/pmx` |
| macOS, Linux | `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kondfox/project-metrics/releases/latest/download/pmx-installer.sh \| sh` |
| Windows (PowerShell) | `powershell -ExecutionPolicy Bypass -c "irm https://github.com/kondfox/project-metrics/releases/latest/download/pmx-installer.ps1 \| iex"` |
| With Rust installed | `cargo binstall pmx` (downloads the same release binary) |

Or download an archive from the [releases page](https://github.com/kondfox/project-metrics/releases).
Building from source is in [CONTRIBUTING.md](../CONTRIBUTING.md). Check the install with `pmx --version`.

## 2. See it on a fictional project (2 minutes, optional)

```sh
pmx demo /tmp/acme && pmx -C /tmp/acme serve --open
```

`demo` writes four fictional repositories with a year of history and collects them. The dashboard opens
in your browser. Ctrl-C stops the server.

## 3. Set up your project (5 minutes)

A project is one **workspace folder**: it holds `pmx.toml` (the project definition), `.pmx/` (the cache)
and `out/` (the results). Your repos can live anywhere: pmx only reads them.

```sh
mkdir -p ~/metrics/acme-shop && cd ~/metrics/acme-shop
pmx init ~/src/acme
```

`pmx init` finds every git clone under the folders you give it and walks you through the setup. Press
Enter to accept a suggestion.

1. **Repos.** For each clone it suggests a branch (the remote's default) and a role, the stack the work
   counts for: `frontend`, `backend`, `mobile`, `qa`, `infra`, `docs`, `data`, or `per-file` for a repo
   that mixes stacks. Type a repo's number to change its role, branch or name, or to drop it.
2. **Project name and first day measured** (default: a year back).
3. **Code-host token.** If the repos are on GitHub or GitLab, pull-request metrics (peer review, cycle
   time, mentoring) need a token with read access in an environment variable (`GITHUB_TOKEN` or
   `GITLAB_TOKEN`). Without one these metrics show as "not measured" and everything else still works.
4. **Velocity classifier.** Velocity scores each piece of work with an LLM. `init` offers only what it
   finds on your machine (the Claude Code CLI, or a running Ollama). The default is none: you can add one
   later, see [velocity.md](velocity.md).
5. **People.** The same person often commits with several emails. `init` groups the identities it finds
   and proposes the `[people]` table. Bots are excluded automatically. Contractors you don't want counted:
   `pmx people external "<name>"`.
6. **Snapshot tools.** `init` offers to download the pinned scc, osv-scanner and gitleaks (checksums
   verified) for complexity, dependency vulnerabilities and secrets.
7. **Check.** `pmx check` verifies paths, branches, the token and identities.

Prefer flags to questions? `pmx init ~/src/acme --yes` writes the suggestions without asking. Then use
`pmx repo set`, `pmx people`, `pmx tools install` and `pmx check` yourself.

## 4. Collect (2–10 minutes)

`init` asks whether to collect right away. Otherwise run:

```sh
pmx collect
```

You see progress and an ETA per stage. The first run reads the whole history; later runs only read what is
new. **Ctrl-C is safe:** run `pmx collect` again and it continues where it stopped. `pmx plan` shows the
work and the ETA without running anything.

## 5. Open the dashboard

```sh
pmx serve --open
```

This serves the dashboard on `http://127.0.0.1:7878`, which only your machine can reach. It includes
the **lead's view** with per-person tables. To share the dashboard, export it:

```sh
pmx export --format html    # out/dashboard.html: one self-contained file, without per-person data
```

What the numbers mean: [metrics overview](../wiki/metrics-overview.md). Read each project against its own
trend. A headline number on its own can mislead.

## Keeping it current

Run `pmx collect` once a week (it fetches first; `--no-fetch` measures what is already there), then
reopen the dashboard. To see several projects side by side, list their workspace folders in a
`fleet.toml` and run `pmx fleet serve --open` ([fleet.md](fleet.md)).

Something not working? See [troubleshooting.md](troubleshooting.md).
