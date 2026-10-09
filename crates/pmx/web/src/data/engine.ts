import type { Metrics, ProjectFile } from "./types";
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

export function engineFrom(call: (request: unknown) => unknown, project: ProjectFile): MetricsEngine {
  call({
    op: "load",
    days: project.days,
    snapshots: project.snapshots,
    breadth_roles: project.project.breadth_roles,
    ai_attribution: project.project.ai_attribution,
  });
  return {
    range: (r) => call({ op: "range", start: formatDate(r.start), end: formatDate(r.end) }) as Metrics,
  };
}

/** `null` when the module is absent (pmx built without the wasm32 target). */
export async function loadEngine(
  wasm: Uint8Array | null,
  project: ProjectFile,
): Promise<MetricsEngine | null> {
  if (!wasm || wasm.length === 0) return null;
  const { instance } = await WebAssembly.instantiate(wasm as BufferSource, {});
  return engineFrom(bridge(instance.exports as unknown as WasmExports), project);
}
