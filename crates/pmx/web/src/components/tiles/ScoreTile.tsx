import type { ReactNode } from "react";
import { formatNumber } from "../../lib/format";
import { metricDef, type MetricId } from "../../metrics/registry";
import { useMeasured, useRangeMetrics } from "../../state/hooks";
import { BandBadge } from "../ui/BandBadge";
import { NotMeasuredTile } from "./NotMeasuredTile";
import { Tile } from "./Tile";
import type { Metrics } from "../../data/types";

/** A 0–100 score with its A–E band (spec §1.7). */
export function ScoreTile(props: {
  id: MetricId;
  label?: string;
  note: (m: Metrics) => ReactNode;
  marker?: string;
}) {
  const r = useRangeMetrics();
  const label = props.label ?? metricDef(props.id).label;
  if (!r) return <Tile label={label} state="empty" note="needs the metrics engine for this range" />;
  const v = r.metrics.values[props.id] ?? null;
  const note = props.note(r.metrics);
  if (v == null) return <Tile label={label} state="empty" note={note} />;
  return (
    <Tile
      label={label}
      value={
        <>
          {props.marker}
          {formatNumber(v)}
          <BandBadge score={v} />
        </>
      }
      note={note}
    />
  );
}

/** Quality with the sub-scores that fed it (spec §3: always show how many, e.g. "3/4"). */
export function QualityTile() {
  const measured = useMeasured();
  return (
    <ScoreTile
      id="quality"
      note={(m) => {
        const v = m.values;
        const subs = (
          [
            ["rework", "s_rework"],
            ["tests", "s_tests"],
            ["docs", "s_docs"],
            ["dup", "s_dup"],
          ] as const
        )
          .map(([label, k]) => `${label} ${v[k] == null ? "–" : formatNumber(v[k])}`)
          .join(" · ");
        const dup = measured("duplication") ? "" : " · duplication not measured";
        return `${formatNumber(v.quality_constituents)}/4 constituents · ${subs}${dup}`;
      }}
    />
  );
}

/** The Security score (spec §7.3): point-in-time, so marked "~"; capped by criticals and secrets. */
export function SecurityTile() {
  const measured = useMeasured();
  if (!measured("dependency_vulnerabilities")) return <NotMeasuredTile id="dependency_vulnerabilities" />;
  return (
    <ScoreTile
      id="security_score"
      marker="~"
      note={(m) => {
        const v = m.values;
        const capped = (v.vuln_critical ?? 0) > 0 || (v.secrets_high ?? 0) > 0;
        const counts = `${formatNumber(v.vuln_critical)} critical · ${formatNumber(v.vuln_high)} high · ${formatNumber(v.vuln_moderate)} moderate`;
        const secrets = measured("secrets") ? ` · ${formatNumber(v.secrets_high)} live secrets` : "";
        return `${counts}${secrets}${capped ? " · capped at 40" : ""}`;
      }}
    />
  );
}
