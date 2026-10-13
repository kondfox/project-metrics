import { useEffect, useRef, useState } from "react";
import { toCsv } from "../../lib/export/csv";
import { download } from "../../lib/export/download";
import { printPage } from "../../lib/export/print";
import type { PageExport } from "../../lib/export/sheet";
import { toXlsx } from "../../lib/export/xlsx";
import styles from "./layout.module.css";

const XLSX = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/** Export what the page shows, with its filters: PDF (print), CSV (the main table) or XLSX (every table). */
export function ExportMenu({ build }: { build: () => PageExport }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !ref.current?.contains(e.target as Node))
        setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);
  const run = (f: () => void) => () => {
    setOpen(false);
    f();
  };
  return (
    <div className={styles.export} ref={ref} data-noprint>
      <button type="button" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        Export ▾
      </button>
      {open && (
        <div className={styles.exportMenu} role="menu">
          <button type="button" role="menuitem" onClick={run(printPage)}>
            PDF <span>print, then “Save as PDF”</span>
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={run(() => {
              const e = build();
              download(toCsv(e.csv.rows), "text/csv;charset=utf-8", `${e.file}.csv`);
            })}
          >
            CSV <span>the main table</span>
          </button>
          <button
            type="button"
            role="menuitem"
            onClick={run(() => {
              const e = build();
              download(toXlsx(e.sheets), XLSX, `${e.file}.xlsx`);
            })}
          >
            Excel (XLSX) <span>every table, one sheet each</span>
          </button>
        </div>
      )}
    </div>
  );
}
