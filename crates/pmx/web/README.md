# pmx dashboard

The project dashboard: React 19 + TypeScript, built by Vite into **one self-contained HTML file** that
`pmx` embeds (`pmx serve`, `pmx export --format html`). The page gets its data from pmx, never from a
server: `project.json`, the optional private `leads.json`, and the metrics engine (WASM).

## Work on it

```sh
npm ci
npm run dev-data   # fictional demo data in dev-data/ (runs `pmx demo`; needs cargo)
npm run dev        # http://localhost:5173, hot reload
npm run check      # typecheck, lint, format check, unit tests (what CI runs)
```

`cargo build -p pmx` runs `npm run build` itself and embeds the result; without Node it embeds a
placeholder page instead. CI sets `PMX_REQUIRE_WEB=1` so that can't happen silently.

## Rules

- **No metric math in TypeScript.** Every number comes from Rust: precomputed weeks and months in
  `project.json`, or any other range from the WASM engine (`data/engine.ts`), which is the same
  `pm-metrics` code as the CLI. A test checks the two agree bit for bit.
- **Types come from Rust.** `src/generated/` is written by `cargo test -p pm-metrics --features ts`
  (ts-rs). Don't edit it; import types from `data/types.ts`. CI fails if it is stale.
- **Three states, never confused:** a value, "no data" (nothing in the range: no data ≠ zero), and "not
  measured" (the input isn't collected yet). Small samples get a low-n badge (spec §1.1).
- **People stay private.** Names only exist in `leads.json`, which shared pages don't carry.

## Layout

| Path                      | What                                                                                                |
| ------------------------- | --------------------------------------------------------------------------------------------------- |
| `src/generated/`          | Rust data model as TypeScript (generated)                                                           |
| `src/data/`               | Loading the injected data (`boot.ts`), the WASM bridge (`engine.ts`), bucket helpers (`project.ts`) |
| `src/lib/`                | Pure helpers: dates and ISO weeks, formatting, range presets, URL hash state                        |
| `src/metrics/registry.ts` | How each metric id is presented: label, unit, description, what `n` counts                          |
| `src/state/`              | `DashboardProvider` (project, engine, cadence, range) and the hooks components use                  |
| `src/components/ui/`      | Primitives: `Badge`, `BandBadge`, `Banner`, `Section`, `SegmentedControl`, `ErrorBoundary`          |
| `src/components/tiles/`   | `Tile` (the three states, low n), `MetricTile`, `QualityTile`, `NotMeasuredTile`, `TileGrid`        |
| `src/components/charts/`  | `EChart` (lifecycle), `buildTimeSeriesOption` (pure, tested), `TimeSeriesChart`, `ChartGrid`        |
| `src/components/tables/`  | Generic typed `DataTable`, `AiCompareTable`, `LeadersTable`                                         |
| `src/components/layout/`  | `Header`, `Controls`, `Notices`, `Footer`                                                           |
| `src/sections/`           | One file per dashboard area, composed from the components above                                     |
| `src/styles/theme.css`    | Design tokens (light and dark); components and charts use only these                                |

## Adding things

- **A metric that pmx already computes:** add it to `METRICS` in `metrics/registry.ts`, then use
  `<MetricTile id="…" />` or a `TimeSeriesChart` series `{ metric: "…" }` in a section.
- **A new metric:** compute it in `pm-metrics` (Rust) with a spec entry first; it then appears in
  `series_*` and in engine responses, and the steps above apply.
- **A new area** (e.g. Security in M3): a new file in `src/sections/`, added to `App.tsx`. Data that
  isn't a series (snapshots, tables) gets a field in the Rust model, regenerate the types, read it
  through `useDashboard()`.
- **A new chart type:** register the ECharts module in `components/charts/EChart.tsx`, write a pure
  option builder next to `timeSeriesOption.ts` with a test, and a small component around it.
