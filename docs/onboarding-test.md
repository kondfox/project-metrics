# Onboarding test (M7 exit criterion)

**Goal:** someone who has never used pmx goes from nothing to their own project's dashboard in under
15 minutes, using only the docs. This page is the protocol. The results go to the plan's "M7 progress"
as times and friction points only, with no names, project names or metric values.

## Setup

- **Tester:** a developer who has not used pmx or read the wiki, on their own machine, with git clones of
  a real project of theirs (any stack, 2+ repos if possible).
- **Start state:** pmx not installed. A code-host token and a classifier are *not* required; note
  whether they had them.
- **They get:** the link to the repo's README. Nothing else. The observer does not help, except to
  unblock after 3 minutes stuck (record it as a failure point).

## Run

Start a timer when they open the README. Record the time at each milestone:

| Milestone | Time |
|---|---|
| pmx installed (`pmx --version` works) | |
| `pmx init` finished (pmx.toml written) | |
| `pmx collect` started | |
| `pmx collect` finished | |
| Dashboard open in the browser with their project's data | |

**Pass:** the dashboard is open within 15 minutes, with no unblocking. `collect` time depends on
history size; if collect alone runs over 5 minutes, record it and judge the rest without it.

## Afterwards (5 minutes)

Ask, and write down the answers:

1. Where did you hesitate or guess?
2. Was any question in `pmx init` unclear? Which defaults did you change, and why?
3. Did anything on the dashboard look wrong or confusing?
4. What would you have needed to set up PR metrics or Velocity?

Turn each friction point into an issue (fictional examples only), fix what blocks the 15 minutes, and
repeat with a different tester until one passes.
