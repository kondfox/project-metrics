const nf = new Intl.NumberFormat("en-US");

/** `12,418`; `–` for no value. */
export function formatNumber(v: number | null | undefined, decimals = 0): string {
  if (v == null || Number.isNaN(v)) return "–";
  return nf.format(Number(v.toFixed(decimals)));
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
