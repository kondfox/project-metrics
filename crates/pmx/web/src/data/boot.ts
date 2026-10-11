import type { FleetFile, LeadsFile, ProjectFile } from "./types";

/** One project's dashboard. */
export interface DashboardData {
  kind: "project";
  project: ProjectFile;
  /** The lead's private view (per-person drill-downs); absent in shared pages. */
  leads: LeadsFile | null;
  /** The metrics engine; null if pmx was built without it. */
  wasm: Uint8Array | null;
}

/** The fleet page (`pmx fleet`): the pooled fleet plus every project's `project.json`. */
export interface FleetData {
  kind: "fleet";
  fleet: FleetFile;
  /** Key (as in `fleet.projects`) → project. Never with `leads.json`: fleet pages are shared. */
  projects: Record<string, ProjectFile>;
  wasm: Uint8Array | null;
}

export type PageData = DashboardData | FleetData;

interface Injected {
  project?: ProjectFile;
  fleet?: FleetFile;
  projects?: Record<string, ProjectFile>;
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

function fromInjected(data: Injected, wasm: Uint8Array | null): PageData {
  if (data.fleet && data.projects) return { kind: "fleet", fleet: data.fleet, projects: data.projects, wasm };
  if (data.project) return { kind: "project", project: data.project, leads: data.leads, wasm };
  throw new Error("this page's data is neither a project nor a fleet");
}

/**
 * pmx writes the data into `<script id="pmx-data">` (one self-contained page). `npm run dev`
 * leaves it empty and fetches fictional demo data from dev-data/ instead (`?fleet` for the fleet).
 */
export async function loadPageData(doc: Document = document): Promise<PageData> {
  const text = doc.getElementById("pmx-data")?.textContent?.trim() ?? "";
  if (text.startsWith("{")) {
    const data = JSON.parse(text) as Injected;
    return fromInjected(data, fromBase64(data.wasm));
  }
  if (!import.meta.env.DEV)
    throw new Error("this page has no data (build it with `pmx export` or `pmx fleet export`)");
  const get = async (file: string) => {
    const r = await fetch(`./${file}`);
    if (!r.ok) throw new Error(`dev-data/${file} is missing: run \`npm run dev-data\``);
    return r;
  };
  const wasm = get("pm_wasm.wasm").then(async (r) => new Uint8Array(await r.arrayBuffer()));
  if (new URLSearchParams(window.location.search).has("fleet")) {
    const data = (await get("fleet-data.json").then((r) => r.json())) as Injected;
    return fromInjected(data, await wasm);
  }
  const [project, leads] = await Promise.all([
    get("project.json").then((r) => r.json() as Promise<ProjectFile>),
    get("leads.json").then((r) => r.json() as Promise<LeadsFile>),
  ]);
  return { kind: "project", project, leads, wasm: await wasm };
}
