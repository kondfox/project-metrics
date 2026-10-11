import type { FleetFile, Metrics, Pooled, ProjectFile } from "./types";
import { formatDate, type DateRange } from "../lib/dates";

/**
 * The metrics engine: pm-metrics compiled to WASM (crates/pm-wasm). It computes any range exactly as
 * the CLI computes a week or a month, so the dashboard never re-implements metric math.
 */
export interface MetricsEngine {
  range(range: DateRange): Metrics;
}

/** The raw exports of pm_wasm.wasm. */
export interface WasmExports {
  memory: WebAssembly.Memory;
  pmx_alloc(len: number): number;
  pmx_free(ptr: number, len: number): void;
  /** Returns `ptr << 32 | len` of the JSON response. */
  pmx_call(ptr: number, len: number): bigint;
}

/** JSON in, JSON out over the module's C ABI. */
export function bridge(ex: WasmExports): (request: unknown) => unknown {
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  return (request) => {
    const bytes = encoder.encode(JSON.stringify(request));
    const ptr = ex.pmx_alloc(bytes.length);
    new Uint8Array(ex.memory.buffer, ptr, bytes.length).set(bytes);
    const packed = ex.pmx_call(ptr, bytes.length);
    ex.pmx_free(ptr, bytes.length);
    const outPtr = Number(packed >> 32n);
    const outLen = Number(packed & 0xffffffffn);
    const text = decoder.decode(new Uint8Array(ex.memory.buffer, outPtr, outLen));
    ex.pmx_free(outPtr, outLen);
    const reply = JSON.parse(text) as { error?: string };
    if (reply.error) throw new Error(`metrics engine: ${reply.error}`);
    return reply;
  };
}

/**
 * Load `project` into the module under `key` (a project page uses ""). `velocityBasis` comes from
 * `fleet.json`: the fleet pools Velocity only across projects with the same one.
 */
export function engineFrom(
  call: (request: unknown) => unknown,
  project: ProjectFile,
  key = "",
  velocityBasis: string | null = null,
): MetricsEngine {
  call({
    op: "load",
    key,
    days: project.days,
    snapshots: project.snapshots,
    breadth_roles: project.project.breadth_roles,
    ai_attribution: project.project.ai_attribution,
    pull_requests: project.meta.pull_requests,
    velocity: project.velocity?.options ?? null,
    velocity_basis: velocityBasis,
  });
  return {
    range: (r) => call({ op: "range", key, start: formatDate(r.start), end: formatDate(r.end) }) as Metrics,
  };
}

async function instantiate(wasm: Uint8Array): Promise<(request: unknown) => unknown> {
  const { instance } = await WebAssembly.instantiate(wasm as BufferSource, {});
  return bridge(instance.exports as unknown as WasmExports);
}

/** `null` when the module is absent (pmx built without the wasm32 target). */
export async function loadEngine(
  wasm: Uint8Array | null,
  project: ProjectFile,
): Promise<MetricsEngine | null> {
  if (!wasm || wasm.length === 0) return null;
  return engineFrom(await instantiate(wasm), project);
}

/** Every project of a fleet in one module, plus the pooled fleet row. */
export interface FleetEngine {
  projects: Record<string, MetricsEngine>;
  pool(range: DateRange): Pooled;
}

export function fleetEngineFrom(
  call: (request: unknown) => unknown,
  fleet: FleetFile,
  projects: Record<string, ProjectFile>,
): FleetEngine {
  const keys = fleet.projects.map((p) => p.key);
  return {
    projects: Object.fromEntries(
      fleet.projects.map((p) => [p.key, engineFrom(call, projects[p.key]!, p.key, p.velocity_basis)]),
    ),
    pool: (r) => call({ op: "fleet", keys, start: formatDate(r.start), end: formatDate(r.end) }) as Pooled,
  };
}

export async function loadFleetEngine(
  wasm: Uint8Array | null,
  fleet: FleetFile,
  projects: Record<string, ProjectFile>,
): Promise<FleetEngine | null> {
  if (!wasm || wasm.length === 0) return null;
  return fleetEngineFrom(await instantiate(wasm), fleet, projects);
}
