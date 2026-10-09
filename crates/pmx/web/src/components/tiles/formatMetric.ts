import { formatNumber, formatPercent } from "../../lib/format";
import type { MetricDef } from "../../metrics/registry";

export function formatMetric(def: MetricDef, v: number | null | undefined): string {
  switch (def.unit) {
    case "percent":
      return formatPercent(v, def.decimals ?? 1);
    default:
      return formatNumber(v, def.decimals ?? 0);
  }
}
