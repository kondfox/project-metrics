const formats = new Map<number, Intl.NumberFormat>();

/**
 * `12,418`; `–` for no value. Keeps exactly `decimals` digits (`3.00`, not `3`), so values of one
 * metric line up in tables and tiles.
 */
export function formatNumber(v: number | null | undefined, decimals = 0): string {
  if (v == null || Number.isNaN(v)) return "–";
  let f = formats.get(decimals);
  if (!f) {
    f = new Intl.NumberFormat("en-US", { minimumFractionDigits: decimals, maximumFractionDigits: decimals });
    formats.set(decimals, f);
  }
  // Rounding first keeps -0.004 from printing as "-0.00".
  return f.format(Number(v.toFixed(decimals)) || 0);
}

export function formatPercent(v: number | null | undefined, decimals = 1): string {
  return v == null || Number.isNaN(v) ? "–" : `${v.toFixed(decimals)}%`;
}

export type Band = "A" | "B" | "C" | "D" | "E";

/** Score bands (spec §1.7). */
export function band(score: number): Band {
  if (score >= 90) return "A";
  if (score >= 75) return "B";
  if (score >= 55) return "C";
  if (score >= 30) return "D";
  return "E";
}
