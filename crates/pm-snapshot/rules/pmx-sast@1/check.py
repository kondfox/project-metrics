#!/usr/bin/env python3
"""Check the pmx-sast rules: semgrep's own tests (`ruleid:` / `ok:` annotations next to each rule
file), plus what those tests can't see: no finding may be reported twice. Run from anywhere:
    python3 crates/pm-snapshot/rules/pmx-sast@1/check.py
Needs semgrep (CI pins the version pmx results are pinned to)."""
import collections, json, pathlib, subprocess, sys

here = pathlib.Path(__file__).resolve().parent
test = subprocess.run(["semgrep", "scan", "--test", "--metrics=off", str(here)], capture_output=True, text=True)
print(test.stdout.strip().splitlines()[0] if test.stdout.strip() else test.stderr)
if test.returncode != 0:
    print(test.stdout, test.stderr)
    sys.exit("semgrep --test failed")

configs = [a for f in sorted(here.glob("*.yaml")) for a in ("--config", str(f))]
targets = [str(p) for p in here.iterdir() if p.suffix != ".yaml" and p.name not in ("README.md", "check.py")]
scan = subprocess.run(["semgrep", "scan", "--metrics=off", "--quiet", "--json", *configs, *targets],
                      capture_output=True, text=True)
results = json.loads(scan.stdout)["results"]
key = lambda r: (r["check_id"].rsplit(".", 1)[-1], r["path"], r["start"]["line"], r["start"]["col"])
dups = [k for k, n in collections.Counter(map(key, results)).items() if n > 1]
if dups:
    sys.exit(f"findings reported more than once: {dups}")
print(f"{len(results)} findings, none duplicated")
