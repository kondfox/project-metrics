import type { ReactNode } from "react";
import { Sparkline } from "../components/charts/Sparkline";
import { formatMetric } from "../components/tiles/formatMetric";
import { Badge } from "../components/ui/Badge";
import { BandBadge } from "../components/ui/BandBadge";
import type { FleetProject, Metrics, Pooled } from "../data/types";
import { band, type Band } from "../lib/format";
import { toHash } from "../lib/urlState";
import { metricDef } from "../metrics/registry";
import tableStyles from "../components/tables/tables.module.css";
import { useFleet } from "./context";
import { HEADLINES, projectMeasures, type Headline } from "./headlines";
import { useFleetRange, useFleetTrend } from "./hooks";
import styles from "./fleet.module.css";

type CellState =
  | { kind: "value"; v: number; approx: boolean }
  | { kind: "empty" }
  | { kind: "notMeasured"; why?: string }
  | { kind: "needsEngine" };

function cellOf(h: Headline, m: Pick<Metrics, "values"> | null, measured: boolean, why?: string): CellState {
  if (!measured) return { kind: "notMeasured", why };
  if (!m) return { kind: "needsEngine" };
  const v = m.values[h.id] ?? null;
  if (v == null) return { kind: "empty" };
  return {
    kind: "value",
    v,
    approx: h.id === "velocity" && m.values.velocity_approx === 1,
  };
}

function Cell({
  h,
  c,
  extra,
  trend,
  was,
}: {
  h: Headline;
  c: CellState;
  extra?: ReactNode;
  trend: Array<number | null>;
  /** The value in the period just before. */
  was: number | null;
}) {
  switch (c.kind) {
    case "notMeasured":
      return (
        <span className={styles.muted} title={c.why}>
          not measured{extra}
        </span>
      );
    case "needsEngine":
      return <span className={styles.muted}>needs the engine</span>;
    case "empty":
      return <span className={styles.muted}>no data{extra}</span>;
    case "value":
      return (
        <>
          {(h.snapshot || c.approx) && "~"}
          {formatMetric(metricDef(h.id), c.v)}
          {h.score && <BandBadge score={c.v} />}
          {extra}
          <Change h={h} now={c.v} was={was} />
          <Sparkline values={trend} label={`${h.label} trend`} />
        </>
      );
  }
}

/** ▲/▼ and how much the value moved since the period just before; green or red when the direction matters. */
function Change({ h, now, was }: { h: Headline; now: number; was: number | null }) {
  if (was == null) return null;
  const def = metricDef(h.id);
  // Percentages move in points, shown whole.
  const decimals = def.unit === "percent" ? 0 : (def.decimals ?? 0);
  const d = now - was;
  if (Number(Math.abs(d).toFixed(decimals)) === 0) return null;
  const up = d > 0;
  const tone = h.higherIsBetter ? (up ? styles.good : styles.bad) : styles.muted;
  return (
    <span
      className={`${styles.change} ${tone}`}
      title={`${up ? "up" : "down"} from ${formatMetric(def, was)} in the period before`}
    >
      {up ? "▲" : "▼"}
      {Math.abs(d).toFixed(decimals)}
    </span>
  );
}

function ProjectName({ p }: { p: FleetProject }) {
  const { cadence, range } = useFleet();
  return (
    <>
      <a
        className={styles.project}
        href={toHash(cadence, range, p.key)}
        title="open this project's dashboard"
      >
        {p.name}
      </a>
      {p.stale && (
        <Badge title="collected before the rest of the fleet: its later days are missing, not zero">
          as of {p.as_of}
        </Badge>
      )}
    </>
  );
}

/** "2/3": how many projects fed a pooled value, when not all did. */
function Coverage({ pooled, h, total }: { pooled: Pooled; h: Headline; total: number }) {
  if (!h.input || h.input === "quality") return null;
  const fed = pooled.fed[h.input]?.length ?? 0;
  if (fed === 0 || fed === total) return null;
  return (
    <span className={styles.small} title={`from ${fed} of ${total} projects (${h.pooling})`}>
      {fed}/{total}
    </span>
  );
}

function fleetCell(h: Headline, pooled: Pooled | null): CellState {
  if (!pooled) return { kind: "needsEngine" };
  const fed = !h.input || h.input === "quality" || (pooled.fed[h.input]?.length ?? 0) > 0;
  const note = h.input ? pooled.notes[h.input] : undefined;
  if (note && pooled.values[h.id] == null) return { kind: "notMeasured", why: note };
  return cellOf(h, pooled, fed, "no project measures it");
}

/** One row per project, then the fleet row; every project links to its own dashboard. */
export function FleetTable() {
  const { fleet, projects } = useFleet();
  const { pooled, rows, previous } = useFleetRange();
  const trend = useFleetTrend();
  const total = fleet.projects.length;
  return (
    <div className={tableStyles.wrap}>
      <table className={`${tableStyles.table} ${styles.table}`} aria-label="Fleet">
        <thead>
          <tr>
            <th scope="col">Project</th>
            {HEADLINES.map((h) => (
              <th key={h.id} scope="col" className={tableStyles.right} title={`fleet row: ${h.pooling}`}>
                {h.label}
                {h.context && <span className={styles.small}>context</span>}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {fleet.projects.map((p) => {
            const project = projects[p.key]!;
            return (
              <tr key={p.key}>
                <th scope="row">
                  <ProjectName p={p} />
                </th>
                {HEADLINES.map((h) => (
                  <td
                    key={h.id}
                    className={tableStyles.right}
                    title={
                      h.id === "velocity" && p.velocity_basis ? `scored by ${p.velocity_basis}` : undefined
                    }
                  >
                    <Cell
                      h={h}
                      c={cellOf(h, rows[p.key] ?? null, projectMeasures(project, h.input))}
                      trend={trend(p.key, h.id)}
                      was={previous?.rows[p.key]?.values[h.id] ?? null}
                    />
                  </td>
                ))}
              </tr>
            );
          })}
          <tr className={styles.fleetRow}>
            <th scope="row">Fleet</th>
            {HEADLINES.map((h) => (
              <td key={h.id} className={tableStyles.right} title={h.pooling}>
                <Cell
                  h={h}
                  c={fleetCell(h, pooled)}
                  trend={trend(null, h.id)}
                  was={previous?.pooled?.values[h.id] ?? null}
                  extra={pooled && <Coverage pooled={pooled} h={h} total={total} />}
                />
              </td>
            ))}
          </tr>
        </tbody>
        <caption>
          <BandCounts rows={rows} />
        </caption>
      </table>
    </div>
  );
}

/** How the projects spread over the bands: the fleet scalar alone can hide one project falling. */
function BandCounts({ rows }: { rows: Record<string, Metrics | null> }) {
  const parts = HEADLINES.filter((h) => h.score).map((h) => {
    const counts = new Map<Band, number>();
    for (const m of Object.values(rows)) {
      const v = m?.values[h.id];
      if (v != null) counts.set(band(v), (counts.get(band(v)) ?? 0) + 1);
    }
    const text = (["A", "B", "C", "D", "E"] as const)
      .filter((b) => counts.has(b))
      .map((b) => `${counts.get(b)}×${b}`)
      .join(" ");
    return `${h.label}: ${text || "–"}`;
  });
  return (
    <span className={styles.bands}>
      Projects by band · {parts.join(" · ")} · ~ point in time or FTE unknown · hover a column for how the
      fleet row is pooled
    </span>
  );
}
