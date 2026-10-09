// Generate fictional demo data for `npm run dev`: runs `pmx demo` and copies its outputs (and the
// WASM engine) into dev-data/, which the dev server serves.
import { execFileSync } from "node:child_process";
import { rmSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const web = dirname(dirname(fileURLToPath(import.meta.url)));
const workspace = join(web, "../../../target/pmx-demo");
rmSync(workspace, { recursive: true, force: true });
execFileSync(
  "cargo",
  ["run", "-q", "--release", "-p", "pmx", "--", "demo", workspace, "--dev-data", join(web, "dev-data")],
  { stdio: "inherit", cwd: web },
);
