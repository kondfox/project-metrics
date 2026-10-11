// Checks that the WASM engine's `fleet` op reproduces every week and month of a fleet.json
// exactly, values and coverage: the fleet page and `pmx fleet` must be one implementation.
// Used by crates/pmx/tests/fleet.rs.
// Usage: node check-fleet-engine.mjs <pm_wasm.wasm> <fleet.json> <key>=<project.json> …
import fs from "node:fs";
const [wasmPath, fleetPath, ...pairs] = process.argv.slice(2);
const F = JSON.parse(fs.readFileSync(fleetPath, "utf8"));
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const ex = instance.exports;
const call = (req) => {
  const enc = new TextEncoder().encode(JSON.stringify(req));
  const p = ex.pmx_alloc(enc.length);
  new Uint8Array(ex.memory.buffer, p, enc.length).set(enc);
  const r = ex.pmx_call(p, enc.length);
  ex.pmx_free(p, enc.length);
  const ptr = Number(r >> 32n),
    len = Number(r & 0xffffffffn);
  const o = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ptr, len)));
  ex.pmx_free(ptr, len);
  if (o.error) throw new Error(o.error);
  return o;
};
for (const pair of pairs) {
  const [key, path] = pair.split("=");
  const P = JSON.parse(fs.readFileSync(path, "utf8"));
  call({
    op: "load",
    key,
    days: P.days,
    snapshots: P.snapshots,
    breadth_roles: P.project.breadth_roles,
    ai_attribution: P.project.ai_attribution,
    pull_requests: P.meta.pull_requests,
    velocity: P.velocity?.options ?? null,
    velocity_basis: F.projects.find((p) => p.key === key).velocity_basis,
  });
}
const keys = F.projects.map((p) => p.key);
const DAY = 864e5,
  f = (d) => d.toISOString().slice(0, 10);
const asOf = new Date(`${F.as_of}T00:00:00Z`);
const month = (l) => {
  const [y, m] = l.split("-").map(Number);
  return [new Date(Date.UTC(y, m - 1, 1)), new Date(Date.UTC(y, m, 0))];
};
const week = (l) => {
  const [y, w] = l.split("-W").map(Number);
  const j = new Date(Date.UTC(y, 0, 4));
  const mo = new Date(j - ((j.getUTCDay() + 6) % 7) * DAY + (w - 1) * 7 * DAY);
  return [mo, new Date(+mo + 6 * DAY)];
};
let checked = 0,
  bad = 0;
const diff = (...msg) => {
  bad++;
  if (bad < 5) console.log("DIFF", ...msg);
};
for (const [labels, series, coverage, rng] of [
  [F.months, F.series_monthly, F.coverage_monthly, month],
  [F.weeks, F.series_weekly, F.coverage_weekly, week],
]) {
  labels.forEach((l, i) => {
    const [s, e] = rng(l);
    const r = call({ op: "fleet", keys, start: f(s), end: f(e < asOf ? e : asOf) });
    for (const k in series) {
      checked++;
      if (series[k][i] !== (r.values[k] ?? null)) diff(l, k, series[k][i], r.values[k]);
    }
    checked++;
    const c = coverage[i];
    if (
      JSON.stringify(c.fed) !== JSON.stringify(r.fed) ||
      JSON.stringify(c.notes) !== JSON.stringify(r.notes)
    )
      diff(l, "coverage", JSON.stringify(c), JSON.stringify({ fed: r.fed, notes: r.notes }));
  });
}
console.log(`compared ${checked} values, ${bad} differ`);
process.exit(bad === 0 && checked > 0 ? 0 : 1);
