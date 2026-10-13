import type { Cell } from "./sheet";

const quote = (c: Cell): string => {
  if (c == null) return "";
  const s = typeof c === "boolean" ? (c ? "yes" : "no") : String(c);
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};

/** RFC 4180, CRLF line ends, with a BOM so Excel reads it as UTF-8. */
export function toCsv(rows: Cell[][]): string {
  return "﻿" + rows.map((r) => r.map(quote).join(",")).join("\r\n") + "\r\n";
}
