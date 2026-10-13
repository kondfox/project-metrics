import { toCsv } from "./csv";
import { fileSlug } from "./sheet";
import { columnName, toXlsx } from "./xlsx";
import { crc32, zipStore } from "./zip";

/** Read back a stored (uncompressed) zip: name → text. */
function unzip(zip: Uint8Array): Record<string, string> {
  const v = new DataView(zip.buffer, zip.byteOffset, zip.byteLength);
  const dec = new TextDecoder();
  const out: Record<string, string> = {};
  let at = 0;
  while (v.getUint32(at, true) === 0x04034b50) {
    const size = v.getUint32(at + 18, true);
    const nameLen = v.getUint16(at + 26, true);
    const name = dec.decode(zip.subarray(at + 30, at + 30 + nameLen));
    const data = zip.subarray(at + 30 + nameLen, at + 30 + nameLen + size);
    expect(crc32(data), name).toBe(v.getUint32(at + 14, true));
    out[name] = dec.decode(data);
    at += 30 + nameLen + size;
  }
  expect(v.getUint32(at, true)).toBe(0x02014b50); // the central directory follows
  return out;
}

describe("export", () => {
  it("writes RFC 4180 CSV that Excel reads as UTF-8", () => {
    expect(
      toCsv([
        ["a", 'say "hi"', null],
        [1.5, "x,y", true],
      ]),
    ).toBe('﻿a,"say ""hi""",\r\n1.5,"x,y",yes\r\n');
  });

  it("computes the zip CRC-32", () => {
    expect(crc32(new TextEncoder().encode("hello"))).toBe(0x3610a686);
  });

  it("zips entries that read back intact", () => {
    const enc = new TextEncoder();
    const files = unzip(zipStore([{ name: "a.txt", data: enc.encode("Árvíztűrő") }]));
    expect(files).toEqual({ "a.txt": "Árvíztűrő" });
  });

  it("names columns like a spreadsheet", () => {
    expect([0, 25, 26, 701, 702].map(columnName)).toEqual(["A", "Z", "AA", "ZZ", "AAA"]);
  });

  it("writes a workbook with well-formed parts, numbers as numbers and one sheet per table", () => {
    const files = unzip(
      toXlsx([
        {
          name: "Summary",
          rows: [
            ["Metric", "Value"],
            ["Quality <score>", 82.5],
            ["Empty", null],
          ],
        },
        { name: "Summary", rows: [["a/b"], [true]] },
      ]),
    );
    for (const [name, xml] of Object.entries(files)) {
      const doc = new DOMParser().parseFromString(xml, "application/xml");
      expect(doc.getElementsByTagName("parsererror"), name).toHaveLength(0);
    }
    expect(files["xl/workbook.xml"]).toContain('<sheet name="Summary" sheetId="1"');
    expect(files["xl/workbook.xml"]).toContain('<sheet name="Summary 2" sheetId="2"');
    const s1 = files["xl/worksheets/sheet1.xml"]!;
    expect(s1).toContain('<c r="B2"><v>82.5</v></c>');
    expect(s1).toContain("Quality &lt;score&gt;");
    expect(s1).not.toContain('r="B3"'); // no data stays empty, never 0
    expect(files["xl/worksheets/sheet2.xml"]).toContain('<c r="A2" t="b"><v>1</v></c>');
  });

  it("makes file names from project names", () => {
    expect(fileSlug("Ünnep Café / Shop 2")).toBe("unnep-cafe-shop-2");
  });
});
