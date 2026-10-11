// Generate fictional demo data for `npm run dev`: runs `pmx demo` and copies its outputs (and the
// WASM engine) into dev-data/, which the dev server serves. `npm run dev-data -- --fleet` makes the
// fictional fleet instead (open the dev server with `?fleet`).
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const web = dirname(dirname(fileURLToPath(import.meta.url)));
const fleet = process.argv.includes("--fleet");
const workspace = join(web, "../../../target", fleet ? "pmx-demo-fleet" : "pmx-demo");
rmSync(workspace, { recursive: true, force: true });
execFileSync(
  "cargo",
  [
    "run",
    "-q",
    "--release",
    "-p",
    "pmx",
    "--",
    "demo",
    workspace,
    "--dev-data",
    join(web, "dev-data"),
    ...(fleet ? ["--fleet"] : []),
  ],
  { stdio: "inherit", cwd: web },
);
