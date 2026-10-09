// pmx project dashboard (plan M2). Plain JS, no build step: pmx inlines this file, ECharts, the
// project data and the pm-metrics WASM module into one HTML page.
//
// window.PMX = { project: <project.json>, leads: <private/leads.json> | null, wasm: <base64> | "" }
"use strict";

(function () {
  const errors = document.getElementById("errors");
  window.addEventListener("error", (e) => showError(e.message || String(e)));
  window.addEventListener("unhandledrejection", (e) => showError(String(e.reason)));
  function showError(msg) {
    errors.hidden = false;
    errors.textContent += msg + "\n";
  }

  const P = window.PMX.project;
  const LEADS = window.PMX.leads;
  const META = P.meta;
  const LOW_N = META.low_n_threshold || 10;
  const DAY = 86400000;

  // ---------- dates and buckets ----------
  const parse = (s) => new Date(s + "T00:00:00Z");
  const fmt = (d) => d.toISOString().slice(0, 10);
  const addDays = (d, n) => new Date(d.getTime() + n * DAY);
  function monthRange(label) {
    const [y, m] = label.split("-").map(Number);
    return [new Date(Date.UTC(y, m - 1, 1)), new Date(Date.UTC(y, m, 0))];
  }
  function weekRange(label) {
    const [y, w] = label.split("-W").map(Number);
    const jan4 = new Date(Date.UTC(y, 0, 4));
    const monday = addDays(jan4, -((jan4.getUTCDay() + 6) % 7) + (w - 1) * 7);
    return [monday, addDays(monday, 6)];
  }
  const bucketRange = (cadence, label) => (cadence === "month" ? monthRange(label) : weekRange(label));

  const AS_OF = parse(META.as_of);
  const START = parse(P.project.range_start);
  const HEADLINE = monthRange(META.headline_month);

  function lastCompleteWeek() {
    const end = addDays(AS_OF, -((AS_OF.getUTCDay() + 0) % 7)); // last Sunday on or before as_of
    return [addDays(end, -6), end];
  }
  function lastMonths(n) {
    const [, end] = HEADLINE;
    const s = new Date(Date.UTC(end.getUTCFullYear(), end.getUTCMonth() - (n - 1), 1));
    return [s < START ? START : s, end];
  }
  const PRESETS = [
    ["month", "Last complete month", () => HEADLINE],
    ["week", "Last complete week", lastCompleteWeek],
    ["3m", "Last 3 months", () => lastMonths(3)],
    ["6m", "Last 6 months", () => lastMonths(6)],
    ["12m", "Last 12 months", () => lastMonths(12)],
    ["all", "Everything", () => [START, AS_OF]],
    ["custom", "Custom…", null],
  ];

  // ---------- WASM metrics engine ----------
  let engine = null;
  async function loadEngine() {
    if (!window.PMX.wasm) return null;
    const bin = atob(window.PMX.wasm);
    const bytes = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
    const { instance } = await WebAssembly.instantiate(bytes, {});
    const ex = instance.exports;
    const call = (req) => {
      const enc = new TextEncoder().encode(JSON.stringify(req));
      const p = ex.pmx_alloc(enc.length);
      new Uint8Array(ex.memory.buffer, p, enc.length).set(enc);
      const r = ex.pmx_call(p, enc.length);
      ex.pmx_free(p, enc.length);
      const ptr = Number(r >> 32n), len = Number(r & 0xffffffffn);
      const out = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ptr, len)));
      ex.pmx_free(ptr, len);
      if (out.error) throw new Error("metrics engine: " + out.error);
      return out;
    };
    call({ op: "load", days: P.days, breadth_roles: P.project.breadth_roles, ai_attribution: P.project.ai_attribution });
    return call;
  }

  /// Metrics for [start, end]: the WASM engine, or a precomputed bucket when the range is one.
  function rangeMetrics(start, end) {
    if (engine) return engine({ op: "range", start: fmt(start), end: fmt(end) });
    for (const cadence of ["month", "week"]) {
      const labels = cadence === "month" ? P.months : P.weeks;
      const i = labels.findIndex((l) => {
        const [s, e] = bucketRange(cadence, l);
        return fmt(s) === fmt(start) && fmt(e) === fmt(end);
      });
      if (i >= 0) {
        const series = cadence === "month" ? P.series_monthly : P.series_weekly;
        const ns = cadence === "month" ? P.n_monthly : P.n_weekly;
        const values = {}, n = {};
        for (const k in series) values[k] = series[k][i];
        for (const k in ns) n[k] = ns[k][i];
        const leaders = LEADS && LEADS.leaders[labels[i]] ? LEADS.leaders[labels[i]] : [];
        return { values, n, stack_mix: (P.detail[labels[i]] || {}).stack_mix || {}, leaders, named: true };
      }
    }
    return null;
  }

  // ---------- formatting ----------
  const nf = new Intl.NumberFormat("en-US");
  const num = (v, d = 0) => (v == null ? "–" : nf.format(Number(v.toFixed(d))));
  const pct = (v, d = 1) => (v == null ? "–" : v.toFixed(d) + "%");
  const band = (s) => (s >= 90 ? "A" : s >= 75 ? "B" : s >= 55 ? "C" : s >= 30 ? "D" : "E");
  const esc = (s) => String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  const ROLE_LABEL = { frontend: "Frontend", backend: "Backend", mobile: "Mobile", qa: "QA", infra: "Infra", data: "Data", docs: "Docs" };

  // ---------- state ----------
  const state = { cadence: "week", preset: "month", start: HEADLINE[0], end: HEADLINE[1] };
  try {
    const saved = JSON.parse(localStorage.getItem("pmx:" + P.project.name) || "null");
    if (saved && saved.cadence) state.cadence = saved.cadence;
  } catch (_) { /* storage may be unavailable */ }
  // A link can carry the view: #weekly|#monthly, optionally &YYYY-MM-DD..YYYY-MM-DD.
  (function fromHash() {
    const [cad, range] = decodeURIComponent(location.hash.slice(1)).split("&");
    if (cad === "weekly" || cad === "monthly") state.cadence = cad === "weekly" ? "week" : "month";
    const m = /^(\d{4}-\d{2}-\d{2})\.\.(\d{4}-\d{2}-\d{2})$/.exec(range || "");
    if (m && m[1] <= m[2]) {
      state.preset = "custom";
      state.start = parse(m[1]);
      state.end = parse(m[2]);
    }
  })();
  function toHash() {
    const h = `#${state.cadence === "week" ? "weekly" : "monthly"}&${fmt(state.start)}..${fmt(state.end)}`;
    if (location.hash !== h) history.replaceState(null, "", h);
  }

  // ---------- tiles ----------
  function tile(t) {
    if (t.nm) {
      return `<div class="tile nm"><div class="label">${esc(t.label)}</div><div class="value">not measured</div><div class="note">${esc(t.nm)}</div></div>`;
    }
    if (t.value == null) {
      return `<div class="tile nodata"><div class="label">${esc(t.label)}</div><div class="value">no data</div><div class="note">${esc(t.empty || "nothing in this range")}</div></div>`;
    }
    const low = t.n != null && t.n < LOW_N;
    const badge = low ? `<span class="badge" title="fewer than ${LOW_N} events: read with care">low n · ${t.n}</span>` : "";
    const b = t.band != null ? `<span class="band ${band(t.band)}">${band(t.band)}</span>` : "";
    return `<div class="tile${low ? " lown" : ""}"><div class="label">${esc(t.label)}${badge}</div>` +
      `<div class="value">${t.text}${b}</div><div class="note">${t.note || ""}</div></div>`;
  }

  function headlineTiles(m) {
    const v = m.values, n = m.n;
    const subs = [["rework", v.s_rework], ["tests", v.s_tests], ["docs", v.s_docs]]
      .map(([k, s]) => `${k} ${s == null ? "–" : Math.round(s)}`).join(" · ");
    const q = v.quality;
    const tiles = [
      { label: "Quality", value: q, text: num(q), band: q, note: `${num(v.quality_constituents)}/4 constituents · ${subs} · duplication not measured` },
      { label: "Multi-stack devs", value: v.multi_stack_pct, n: n.multi_stack_pct, text: pct(v.multi_stack_pct), note: `of ${num(n.multi_stack_pct)} active devs` },
      P.project.ai_attribution
        ? { label: "AI-assisted commits", value: v.ai_assist_commit_pct, n: n.ai_assist_commit_pct, text: pct(v.ai_assist_commit_pct), note: `${pct(v.ai_assist_line_pct)} of added lines` }
        : { label: "AI-assisted", nm: "not tracked: AI trailers are stripped by policy" },
      { label: "Velocity", nm: "needs the LLM classifier (M5)" },
      { label: "Security", nm: "needs osv-scanner and gitleaks snapshots (M3)" },
      { label: "Peer review", nm: "needs code-host data (M4)" },
    ];
    return tiles.map(tile).join("");
  }

  function qualityTiles(m) {
    const v = m.values, n = m.n;
    return [
      { label: "Rework", value: v.rework_pct, n: n.rework_pct, text: pct(v.rework_pct), note: `target band 15–25% · score ${num(v.s_rework)}`, empty: "no lines added" },
      { label: "Tests with code", value: v.test_discipline_pct, n: n.test_discipline_pct, text: pct(v.test_discipline_pct), note: `of ${num(n.test_discipline_pct)} commits touching prod · score ${num(v.s_tests)}` },
      { label: "Docs with code", value: v.doc_discipline_pct, n: n.doc_discipline_pct, text: pct(v.doc_discipline_pct), note: `of ${num(n.doc_discipline_pct)} commits touching prod · score ${num(v.s_docs)}` },
      { label: "Duplication", nm: "needs jscpd snapshots (M3)" },
    ].map(tile).join("");
  }

  function activityTiles(m) {
    const v = m.values, n = m.n;
    return [
      { label: "Commits", value: v.commits, text: num(v.commits) },
      { label: "Lines added", value: v.added, text: num(v.added), note: "source files" },
      { label: "Commit size", value: v.commit_med, n: n.commit_med, text: num(v.commit_med), note: `median lines · p90 ${num(v.commit_p90)}` },
      { label: "Commits per dev", value: v.commits_per_dev_med, text: num(v.commits_per_dev_med, 1), note: `median of ${num(n.commits_per_dev_med)} people` },
      { label: "Active devs", value: v.active_devs, text: num(v.active_devs), note: "added lines in a breadth role" },
    ].map(tile).join("");
  }

  function stackTiles(m) {
    const v = m.values, n = m.n;
    return [
      { label: "Multi-stack devs", value: v.multi_stack_pct, n: n.multi_stack_pct, text: pct(v.multi_stack_pct), note: "≥ 40 lines in 2+ roles" },
      { label: "Breadth index", value: v.breadth_index, n: n.breadth_index, text: num(v.breadth_index, 1), note: "mean normalized entropy × 100" },
      { label: "Techs per dev", value: v.techs_per_dev, n: n.techs_per_dev, text: num(v.techs_per_dev, 2), note: "technologies with ≥ 40 lines" },
    ].map(tile).join("");
  }

  // ---------- charts ----------
  const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  const PALETTE = () => ["--c1", "--c2", "--c3", "--c4", "--c5", "--c6", "--c7"].map(css);
  const charts = [];

  function chartWindow() {
    const yearBack = addDays(state.end, -365);
    const from = state.start < yearBack ? state.start : yearBack;
    const labels = state.cadence === "month" ? P.months : P.weeks;
    const idx = [];
    labels.forEach((l, i) => {
      const [s, e] = bucketRange(state.cadence, l);
      if (e >= from && s <= state.end) idx.push(i);
    });
    return { labels, idx };
  }

  function seriesData(metric, idx) {
    const series = state.cadence === "month" ? P.series_monthly : P.series_weekly;
    const ns = state.cadence === "month" ? P.n_monthly : P.n_weekly;
    const vals = series[metric] || [];
    const n = ns[metric] || [];
    return idx.map((i) => {
      const v = vals[i];
      if (v == null) return null;
      const low = n[i] != null && n[i] < LOW_N;
      return low ? { value: v, symbol: "emptyCircle", symbolSize: 7, itemStyle: { opacity: 0.45 }, n: n[i] } : { value: v, n: n[i] };
    });
  }

  function chart(container, spec) {
    const div = document.createElement("div");
    div.className = "chart";
    div.innerHTML = `<h3>${esc(spec.title)}</h3>${spec.hint ? `<div class="hint">${esc(spec.hint)}</div>` : ""}<div class="plot"></div>`;
    container.appendChild(div);
    const { labels, idx } = chartWindow();
    const partial = state.cadence === "month" ? META.partial_month : META.partial_week;
    const xs = idx.map((i) => labels[i] + (partial && i === labels.length - 1 ? "*" : ""));
    const colors = PALETTE();
    const inRange = idx.map((i) => {
      const [s, e] = bucketRange(state.cadence, labels[i]);
      return e >= state.start && s <= state.end;
    });
    const first = inRange.indexOf(true), last = inRange.lastIndexOf(true);
    const ink = css("--ink"), muted = css("--muted"), line = css("--line");
    const series = spec.series.map((s, k) => ({
      name: s.name,
      type: s.type || "line",
      stack: s.stack,
      yAxisIndex: s.axis || 0,
      data: s.data || seriesData(s.metric, idx),
      connectNulls: false,
      showSymbol: true,
      symbolSize: 5,
      lineStyle: { width: s.bold ? 3 : 1.6 },
      color: colors[(s.color ?? k) % colors.length],
      barMaxWidth: 18,
      markArea: k === 0 && first >= 0
        ? {
            silent: true,
            itemStyle: { color: css("--faint"), opacity: 0.6 },
            data: [
              ...(spec.band ? [[{ yAxis: spec.band[0], itemStyle: { color: css("--good"), opacity: 0.08 } }, { yAxis: spec.band[1] }]] : []),
              ...(last - first + 1 < xs.length ? [[{ xAxis: xs[first] }, { xAxis: xs[last] }]] : []),
            ],
          }
        : undefined,
    }));
    // No axis names: titles and hints carry the units, and names collide with the legend.
    const axis = (_name, max) => ({
      type: "value", max, axisLabel: { color: muted },
      splitLine: { lineStyle: { color: line } },
    });
    const c = echarts.init(div.querySelector(".plot"), null, { renderer: "svg" });
    c.setOption({
      animation: false,
      // The right padding leaves room for half of the last centered x label.
      grid: { left: 8, right: 30, top: 30, bottom: 4, containLabel: true },
      legend: { top: 0, textStyle: { color: ink }, itemWidth: 14, itemHeight: 8 },
      tooltip: {
        trigger: "axis",
        valueFormatter: (v) => (v == null ? "no data" : spec.unit === "%" ? v.toFixed(1) + "%" : num(v, spec.decimals || 0)),
      },
      xAxis: {
        type: "category", data: xs, axisLine: { lineStyle: { color: line } },
        axisLabel: { color: muted },
      },
      yAxis: [axis(spec.y || "", spec.max), ...(spec.y2 ? [axis(spec.y2)] : [])],
      series,
    });
    charts.push(c);
  }

  // ---------- tables ----------
  function aiCompare() {
    const a = P.ai_compare;
    if (!P.project.ai_attribution) return `<p class="range-label">AI-assisted work is not tracked for this project (trailers are stripped by policy).</p>`;
    if (!a) return `<p class="range-label">No commits to compare.</p>`;
    const row = (k, label, f) => `<tr><td>${label}</td><td class="num">${f(a.ai[k])}</td><td class="num">${f(a.human[k])}</td></tr>`;
    return `<table><thead><tr><th>${esc(a.from)} – ${esc(a.to)}</th><th class="num">AI-assisted</th><th class="num">Human</th></tr></thead><tbody>` +
      row("commits", "Commits", num) + row("added", "Lines added", num) + row("med_size", "Median commit size", num) +
      row("test_pct", "Tests with code", pct) + row("doc_pct", "Docs with code", pct) +
      `</tbody></table><p class="range-label">Project level only, never per developer (spec §6).</p>`;
  }

  function leadersTable(m) {
    if (!LEADS) return "";
    const named = m.named ? m.leaders : (m.leaders || []).map((l) => ({ ...l, name: LEADS.people[l.name] || l.name }));
    if (!named.length) return `<p class="range-label">No one added lines in a breadth role in this range.</p>`;
    const rows = named.map((l) => `<tr><td>${esc(l.name)}</td><td>${l.stacks.map((s) => ROLE_LABEL[s] || s).join(", ") || "–"}</td>` +
      `<td>${esc(l.techs.join(", ") || "–")}</td><td class="num">${(l.breadth * 100).toFixed(0)}</td><td class="num">${num(l.lines)}</td></tr>`).join("");
    return `<table class="private"><thead><tr><th>Person (private: lead only)</th><th>Stacks ≥ 40 lines</th><th>Technologies</th><th class="num">Breadth</th><th class="num">Lines</th></tr></thead><tbody>${rows}</tbody></table>`;
  }

  // ---------- page ----------
  function render() {
    toHash();
    charts.forEach((c) => c.dispose());
    charts.length = 0;
    const m = rangeMetrics(state.start, state.end);
    const app = document.getElementById("app");
    const repos = P.project.repos;
    const presetOpts = PRESETS.map(([k, label]) => `<option value="${k}"${k === state.preset ? " selected" : ""}>${label}</option>`).join("");
    const banners = [];
    if (META.dialect !== "spec") banners.push(`This file was produced in the <b>${esc(META.dialect)}</b> dialect (parity harness), not by a normal run.`);
    if (!engine) banners.push("Custom ranges need the metrics engine (WASM), which this build of pmx lacks: only whole weeks and months can be shown.");
    app.innerHTML = `
      <header>
        <h1>${esc(P.project.name)}</h1>
        <span class="sub">as of ${esc(META.as_of)}${META.partial_month ? " (month in progress)" : ""} · ${repos.length} repo${repos.length === 1 ? "" : "s"} · since ${esc(P.project.range_start)}</span>
      </header>
      ${banners.map((b) => `<div class="banner">${b}</div>`).join("")}
      <div class="controls">
        <span class="seg"><button data-cad="week" class="${state.cadence === "week" ? "on" : ""}">Weekly</button><button data-cad="month" class="${state.cadence === "month" ? "on" : ""}">Monthly</button></span>
        <select id="preset">${presetOpts}</select>
        <input type="date" id="from" value="${fmt(state.start)}" min="${fmt(START)}" max="${fmt(AS_OF)}">
        <input type="date" id="to" value="${fmt(state.end)}" min="${fmt(START)}" max="${fmt(AS_OF)}">
        <span class="range-label">tiles: ${fmt(state.start)} – ${fmt(state.end)} · charts: ${state.cadence === "week" ? "weeks" : "months"} up to the range end; * = in progress; hollow = low n</span>
      </div>
      ${m ? `
      <section><h2>Headline</h2><div class="tiles">${headlineTiles(m)}</div></section>
      <section><h2>Quality</h2><div class="tiles">${qualityTiles(m)}</div><div class="charts" id="c-quality"></div></section>
      <section><h2>Activity</h2><div class="tiles">${activityTiles(m)}</div><div class="charts" id="c-activity"></div></section>
      <section><h2>Multi-stack</h2><div class="tiles">${stackTiles(m)}</div><div class="charts" id="c-stack"></div>${leadersTable(m)}</section>
      <section><h2>AI-assisted vs human (trailing 12 months)</h2>${aiCompare()}<div class="charts" id="c-ai"></div></section>
      ` : `<div class="banner">This range is not a whole week or month, and custom ranges need the metrics engine.</div>`}
      <footer>
        ${repos.map((r) => `<div>${esc(r.name)} · ${esc(r.role)} · <code>${esc(r.sha.slice(0, 12))}</code> · ${esc(r.tip_date)}</div>`).join("")}
        <div>Generated ${esc(META.generated_at)} by pmx ${esc(META.tool_versions.pmx || "")} · ${esc(META.tool_versions.git || "")}${LEADS ? " · private lead view (leads.json loaded)" : ""}</div>
      </footer>`;

    app.querySelectorAll("[data-cad]").forEach((b) => b.addEventListener("click", () => {
      state.cadence = b.dataset.cad;
      try { localStorage.setItem("pmx:" + P.project.name, JSON.stringify({ cadence: state.cadence })); } catch (_) { /* ignore */ }
      render();
    }));
    app.querySelector("#preset").addEventListener("change", (e) => {
      state.preset = e.target.value;
      const p = PRESETS.find((x) => x[0] === state.preset);
      if (p[2]) [state.start, state.end] = p[2]();
      render();
    });
    for (const id of ["from", "to"]) {
      app.querySelector("#" + id).addEventListener("change", () => {
        const s = parse(app.querySelector("#from").value), e = parse(app.querySelector("#to").value);
        if (isNaN(s) || isNaN(e) || s > e) return;
        state.preset = "custom";
        state.start = s;
        state.end = e;
        render();
      });
    }
    if (!m) return;

    const mixRoles = Object.keys(P.series_monthly).filter((k) => k.startsWith("mix_")).map((k) => k.slice(4));
    const q = document.getElementById("c-quality");
    chart(q, { title: "Quality score", hint: "geometric mean of the sub-scores present; 100 = on target", max: 100, series: [
      { name: "Quality", metric: "quality", bold: true },
      { name: "Rework", metric: "s_rework" }, { name: "Tests", metric: "s_tests" }, { name: "Docs", metric: "s_docs" }] });
    chart(q, { title: "Rework %", hint: "lines deleted within 21 days of being added; shaded = 15–25% target band", unit: "%", band: [15, 25], series: [
      { name: "Rework", metric: "rework_pct" }] });
    chart(q, { title: "Tests and docs with code %", hint: "commits touching prod that also touch tests / docs", unit: "%", max: 100, series: [
      { name: "Tests", metric: "test_discipline_pct" }, { name: "Docs", metric: "doc_discipline_pct", color: 1 }] });
    const a = document.getElementById("c-activity");
    chart(a, { title: "Commits and lines added", y: "commits", y2: "lines", series: [
      { name: "Commits", metric: "commits", type: "bar" }, { name: "Lines added", metric: "added", axis: 1, color: 1 }] });
    chart(a, { title: "Commit size (lines)", hint: "added + deleted source lines per commit", series: [
      { name: "Median", metric: "commit_med" }, { name: "p90", metric: "commit_p90", color: 1 }] });
    chart(a, { title: "Stack mix %", hint: "share of added source lines by role", unit: "%", max: 100, series:
      mixRoles.map((r, k) => ({ name: ROLE_LABEL[r] || r, metric: "mix_" + r, type: "bar", stack: "mix", color: k })) });
    const s = document.getElementById("c-stack");
    chart(s, { title: "Multi-stack devs % and active devs", unit: "%", y2: "devs", series: [
      { name: "Multi-stack %", metric: "multi_stack_pct" }, { name: "Breadth index", metric: "breadth_index", color: 2 },
      { name: "Active devs", metric: "active_devs", type: "bar", axis: 1, color: 6 }] });
    if (P.project.ai_attribution) {
      chart(document.getElementById("c-ai"), { title: "AI-assisted %", hint: "Co-Authored-By: Claude trailer", unit: "%", max: 100, series: [
        { name: "Commits", metric: "ai_assist_commit_pct" }, { name: "Lines", metric: "ai_assist_line_pct", color: 1 }] });
    }
  }

  window.addEventListener("resize", () => charts.forEach((c) => c.resize()));
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", render);

  loadEngine()
    .then((e) => { engine = e; })
    .catch((e) => showError("metrics engine failed to load: " + e.message))
    .finally(() => {
      render();
      document.body.dataset.ready = "1";
    });
})();
