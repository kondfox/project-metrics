import { formatNumber } from "../../lib/format";
import { useRangeMetrics } from "../../state/hooks";
import { BandBadge } from "../ui/BandBadge";
import { Tile } from "./Tile";

/** The Quality score with its band and the sub-scores that fed it (spec §3: show "3/4"). */
export function QualityTile() {
  const r = useRangeMetrics();
  if (!r) return <Tile label="Quality" state="empty" note="needs the metrics engine for this range" />;
  const v = r.metrics.values;
  const q = v.quality ?? null;
  const subs = (["s_rework", "s_tests", "s_docs"] as const)
    .map((k) => `${k.slice(2)} ${v[k] == null ? "–" : formatNumber(v[k])}`)
    .join(" · ");
  const note = `${formatNumber(v.quality_constituents)}/4 constituents · ${subs} · duplication not measured`;
  if (q == null) return <Tile label="Quality" state="empty" note={note} />;
  return (
    <Tile
      label="Quality"
      value={
        <>
          {formatNumber(q)}
          <BandBadge score={q} />
        </>
      }
      note={note}
    />
  );
}
