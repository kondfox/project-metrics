import type { LeadsFile, ProjectFile } from "./types";

export interface DashboardData {
  project: ProjectFile;
  /** The lead's private view (per-person drill-downs); absent in shared pages. */
  leads: LeadsFile | null;
  /** The metrics engine; null if pmx was built without it. */
  wasm: Uint8Array | null;
}

interface Injected {
  project: ProjectFile;
  leads: LeadsFile | null;
  /** base64 */
  wasm: string;
}

function fromBase64(b64: string): Uint8Array | null {
  if (!b64) return null;
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/**
 * pmx writes the data into `<script id="pmx-data">` (one self-contained page). `npm run dev`
 * leaves it empty and fetches fictional demo data from dev-data/ instead.
 */
export async function loadDashboardData(doc: Document = document): Promise<DashboardData> {
  const text = doc.getElementById("pmx-data")?.textContent?.trim() ?? "";
  if (text.startsWith("{")) {
    const data = JSON.parse(text) as Injected;
    return { project: data.project, leads: data.leads, wasm: fromBase64(data.wasm) };
  }
  if (!import.meta.env.DEV) throw new Error("this page has no project data (build it with `pmx export`)");
  const get = async (file: string) => {
    const r = await fetch(`./${file}`);
    if (!r.ok) throw new Error(`dev-data/${file} is missing: run \`npm run dev-data\``);
    return r;
  };
  const [project, leads, wasm] = await Promise.all([
    get("project.json").then((r) => r.json() as Promise<ProjectFile>),
    get("leads.json").then((r) => r.json() as Promise<LeadsFile>),
    get("pm_wasm.wasm").then((r) => r.arrayBuffer()),
  ]);
  return { project, leads, wasm: new Uint8Array(wasm) };
}
