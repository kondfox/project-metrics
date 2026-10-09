// Checks that the WASM engine reproduces every week and month of a project.json exactly: the
// dashboard and the CLI must be one implementation (plan §2.2). Used by crates/pmx/tests/dashboard.rs.
// Usage: node check-engine.mjs <pm_wasm.wasm> <project.json>   (exit 1 on any difference)
import fs from 'node:fs';
const [wasmPath, projPath] = process.argv.slice(2);
const P = JSON.parse(fs.readFileSync(projPath, 'utf8'));
const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});
const ex = instance.exports;
const call = (req) => {
  const enc = new TextEncoder().encode(JSON.stringify(req)); const p = ex.pmx_alloc(enc.length);
  new Uint8Array(ex.memory.buffer, p, enc.length).set(enc); const r = ex.pmx_call(p, enc.length); ex.pmx_free(p, enc.length);
  const ptr = Number(r >> 32n), len = Number(r & 0xffffffffn);
  const o = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ptr, len))); ex.pmx_free(ptr, len); return o;
};
call({ op: 'load', days: P.days, breadth_roles: P.project.breadth_roles, ai_attribution: P.project.ai_attribution });
const DAY = 864e5, f = (d) => d.toISOString().slice(0, 10);
const month = (l) => { const [y, m] = l.split('-').map(Number); return [new Date(Date.UTC(y, m - 1, 1)), new Date(Date.UTC(y, m, 0))]; };
const week = (l) => { const [y, w] = l.split('-W').map(Number); const j = new Date(Date.UTC(y, 0, 4)); const mo = new Date(j - ((j.getUTCDay() + 6) % 7) * DAY + (w - 1) * 7 * DAY); return [mo, new Date(+mo + 6 * DAY)]; };
let checked = 0, bad = 0;
for (const [labels, series, rng] of [[P.months, P.series_monthly, month], [P.weeks, P.series_weekly, week]]) {
  labels.forEach((l, i) => {
    const [s, e] = rng(l); const r = call({ op: 'range', start: f(s), end: f(e) });
    for (const k in series) { checked++; if (series[k][i] !== r.values[k]) { bad++; if (bad < 5) console.log('DIFF', l, k, series[k][i], r.values[k]); } }
  });
}
console.log(`compared ${checked} values, ${bad} differ`);
process.exit(bad === 0 && checked > 0 ? 0 : 1);
