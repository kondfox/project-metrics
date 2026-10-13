import type { Cell, Sheet } from "./sheet";
import { zipStore } from "./zip";

/** A minimal SpreadsheetML workbook: inline strings, numbers, a bold frozen header row. */

const NS = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL = "http://schemas.openxmlformats.org/package/2006/relationships";
const HEAD = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n';

/** XML 1.0 allows tab, newline, carriage return and everything from space up, bar lone surrogates and U+FFFE/F. */
const xmlChar = (ch: string) => {
  const c = ch.codePointAt(0)!;
  const lone = c >= 0xd800 && c <= 0xdfff;
  return c === 9 || c === 10 || c === 13 || (c >= 0x20 && !lone && c !== 0xfffe && c !== 0xffff);
};

// Characters XML forbids are dropped; the rest escaped.
const esc = (s: string) =>
  [...s]
    .filter(xmlChar)
    .join("")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");

/** 0 → A, 25 → Z, 26 → AA. */
export function columnName(i: number): string {
  let s = "";
  for (let n = i + 1; n > 0; n = Math.floor((n - 1) / 26)) s = String.fromCharCode(65 + ((n - 1) % 26)) + s;
  return s;
}

/** Excel's sheet-name rules: ≤ 31 characters, none of []:*?/\, unique. */
function sheetNames(sheets: Sheet[]): string[] {
  const used = new Set<string>();
  return sheets.map((s) => {
    const base = s.name.replace(/[[\]:*?/\\]/g, " ").slice(0, 31) || "Sheet";
    let name = base;
    for (let k = 2; used.has(name.toLowerCase()); k++) name = `${base.slice(0, 28)} ${k}`;
    used.add(name.toLowerCase());
    return name;
  });
}

function cellXml(c: Cell, ref: string, header: boolean): string {
  if (c == null || c === "") return "";
  const style = header ? ' s="1"' : "";
  if (typeof c === "number" && Number.isFinite(c)) return `<c r="${ref}"${style}><v>${c}</v></c>`;
  if (typeof c === "boolean") return `<c r="${ref}" t="b"${style}><v>${c ? 1 : 0}</v></c>`;
  return `<c r="${ref}" t="inlineStr"${style}><is><t xml:space="preserve">${esc(String(c))}</t></is></c>`;
}

function sheetXml(s: Sheet): string {
  const width = Math.max(1, ...s.rows.map((r) => r.length));
  const cols = Array.from({ length: width }, (_, i) => {
    const longest = Math.max(4, ...s.rows.map((r) => String(r[i] ?? "").length));
    return `<col min="${i + 1}" max="${i + 1}" width="${Math.min(60, longest + 2)}" customWidth="1"/>`;
  }).join("");
  const rows = s.rows
    .map(
      (r, ri) =>
        `<row r="${ri + 1}">${r.map((c, ci) => cellXml(c, `${columnName(ci)}${ri + 1}`, ri === 0)).join("")}</row>`,
    )
    .join("");
  return (
    `${HEAD}<worksheet xmlns="${NS}"><sheetViews><sheetView workbookViewId="0">` +
    `<pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>` +
    `<cols>${cols}</cols><sheetData>${rows}</sheetData></worksheet>`
  );
}

const STYLES =
  `${HEAD}<styleSheet xmlns="${NS}">` +
  `<fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/></font></fonts>` +
  `<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>` +
  `<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>` +
  `<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>` +
  `<cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/>` +
  `<xf numFmtId="0" fontId="1" fillId="0" borderId="0" xfId="0" applyFont="1"/></cellXfs>` +
  `<cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>`;

export function toXlsx(sheets: Sheet[], now = new Date()): Uint8Array<ArrayBuffer> {
  const enc = new TextEncoder();
  const names = sheetNames(sheets);
  const n = sheets.length;
  const files: Record<string, string> = {
    "[Content_Types].xml":
      `${HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">` +
      `<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>` +
      `<Default Extension="xml" ContentType="application/xml"/>` +
      `<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>` +
      `<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>` +
      sheets
        .map(
          (_, i) =>
            `<Override PartName="/xl/worksheets/sheet${i + 1}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>`,
        )
        .join("") +
      `</Types>`,
    "_rels/.rels":
      `${HEAD}<Relationships xmlns="${PKG_REL}">` +
      `<Relationship Id="rId1" Type="${REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>`,
    "xl/workbook.xml":
      `${HEAD}<workbook xmlns="${NS}" xmlns:r="${REL}"><sheets>` +
      names.map((name, i) => `<sheet name="${esc(name)}" sheetId="${i + 1}" r:id="rId${i + 1}"/>`).join("") +
      `</sheets></workbook>`,
    "xl/_rels/workbook.xml.rels":
      `${HEAD}<Relationships xmlns="${PKG_REL}">` +
      sheets
        .map(
          (_, i) =>
            `<Relationship Id="rId${i + 1}" Type="${REL}/worksheet" Target="worksheets/sheet${i + 1}.xml"/>`,
        )
        .join("") +
      `<Relationship Id="rId${n + 1}" Type="${REL}/styles" Target="styles.xml"/></Relationships>`,
    "xl/styles.xml": STYLES,
  };
  sheets.forEach((s, i) => (files[`xl/worksheets/sheet${i + 1}.xml`] = sheetXml(s)));
  return zipStore(
    Object.entries(files).map(([name, xml]) => ({ name, data: enc.encode(xml) })),
    now,
  );
}
