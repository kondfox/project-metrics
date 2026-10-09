import { formatDate, isValidDate, parseDate, type Cadence, type DateRange } from "./dates";

/** The view in the URL hash, so a link opens the same view: `#monthly&2026-06-15..2026-09-30`. */
export interface UrlState {
  cadence?: Cadence;
  range?: DateRange;
}

export function parseHash(hash: string): UrlState {
  const [cad, range] = decodeURIComponent(hash.replace(/^#/, "")).split("&");
  const out: UrlState = {};
  if (cad === "weekly") out.cadence = "week";
  if (cad === "monthly") out.cadence = "month";
  const m = /^(\d{4}-\d{2}-\d{2})\.\.(\d{4}-\d{2}-\d{2})$/.exec(range ?? "");
  if (m) {
    const start = parseDate(m[1]!);
    const end = parseDate(m[2]!);
    if (isValidDate(start) && isValidDate(end) && start <= end) out.range = { start, end };
  }
  return out;
}

export const toHash = (cadence: Cadence, range: DateRange): string =>
  `#${cadence === "week" ? "weekly" : "monthly"}&${formatDate(range.start)}..${formatDate(range.end)}`;
