import { formatDate, isValidDate, parseDate, type Cadence, type DateRange } from "./dates";

/**
 * The view in the URL hash, so a link opens the same view: `#monthly&2026-06-15..2026-09-30`. On a
 * fleet page, `p=<key>` first opens that project's dashboard: `#p=shop&monthly&…`.
 */
export interface UrlState {
  project?: string;
  cadence?: Cadence;
  range?: DateRange;
}

export function parseHash(hash: string): UrlState {
  const out: UrlState = {};
  for (const part of decodeURIComponent(hash.replace(/^#/, "")).split("&")) {
    if (part === "weekly") out.cadence = "week";
    else if (part === "monthly") out.cadence = "month";
    else if (part.startsWith("p=") && part.length > 2) out.project = part.slice(2);
    else {
      const m = /^(\d{4}-\d{2}-\d{2})\.\.(\d{4}-\d{2}-\d{2})$/.exec(part);
      if (!m) continue;
      const start = parseDate(m[1]!);
      const end = parseDate(m[2]!);
      if (isValidDate(start) && isValidDate(end) && start <= end) out.range = { start, end };
    }
  }
  return out;
}

export const toHash = (cadence: Cadence, range: DateRange, project?: string | null): string =>
  `#${project ? `p=${encodeURIComponent(project)}&` : ""}${cadence === "week" ? "weekly" : "monthly"}&${formatDate(range.start)}..${formatDate(range.end)}`;
