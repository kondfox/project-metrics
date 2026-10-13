/** One cell: a number stays a number in XLSX; null is an empty cell (no data ≠ zero). */
export type Cell = string | number | boolean | null;

/** A table to export: the first row is the header. */
export interface Sheet {
  name: string;
  rows: Cell[][];
}

/** What a page exports: CSV takes one table, XLSX every sheet. */
export interface PageExport {
  /** File name without extension. */
  file: string;
  csv: Sheet;
  sheets: Sheet[];
}

/** A file-name-safe slug: letters and digits kept (accents dropped), the rest → "-". */
export function fileSlug(s: string): string {
  return (
    s
      .normalize("NFKD")
      .replace(/[̀-ͯ]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-|-$/g, "") || "export"
  );
}
