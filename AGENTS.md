# AGENTS.md

- `wiki/` is an LLM wiki and the design source of truth. Start at `wiki/README.md`.
  - `wiki/metrics-spec.md` defines every metric and wins over the topic pages.
  - `wiki/tool-implementation-plan.md` defines how the tool is built (architecture, milestones).
- When a decision is made, record it in the relevant wiki page (decision tables carry dates).
- This is a public, open-source repo: never write real people, client or project names, emails, internal
  hosts or metric outputs into it. Use fictional examples.
- Status: M1 implemented (Rust workspace in `crates/`, see the plan's milestones). Don't start a new
  milestone unless asked.
- Parity runs read the private golden set via `PMX_GOLDEN_DIR` and print to the terminal only; nothing
  from the golden set or its reports goes into this repo.
