import { type ReactNode, useLayoutEffect, useRef } from "react";
import styles from "./tables.module.css";

export interface Column<T> {
  header: ReactNode;
  cell(row: T): ReactNode;
  align?: "left" | "right";
  key?: string;
}

/** Cap a scrolling wrapper at the header plus the first `n` rows, measured, so wrapped rows count. */
function useVisibleRows(n: number | undefined, deps: unknown) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !n) return;
    const measure = () => {
      const rows = el.querySelectorAll("tbody tr");
      if (rows.length <= n) {
        el.style.maxHeight = "";
        return;
      }
      let h = el.querySelector("thead")?.getBoundingClientRect().height ?? 0;
      for (const row of Array.from(rows).slice(0, n)) h += row.getBoundingClientRect().height;
      el.style.maxHeight = `${Math.ceil(h)}px`;
    };
    measure();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(measure);
    const table = el.querySelector("table");
    if (table) ro.observe(table);
    return () => ro.disconnect();
  }, [n, deps]);
  return ref;
}

/** A plain, typed table. `private` marks lead-only content. `visibleRows` scrolls the body inside a
 * box that shows that many rows, with the header kept in view and the caption below the box. */
export function DataTable<T>(props: {
  columns: ReadonlyArray<Column<T>>;
  rows: ReadonlyArray<T>;
  rowKey(row: T, index: number): string;
  caption?: ReactNode;
  label?: string;
  variant?: "default" | "private";
  visibleRows?: number;
}) {
  const right = (c: Column<T>) => (c.align === "right" ? styles.right : undefined);
  const scroll = props.visibleRows != null;
  const ref = useVisibleRows(props.visibleRows, props.rows);
  return (
    <>
      <div ref={ref} className={scroll ? `${styles.wrap} ${styles.scroll}` : styles.wrap}>
        <table
          className={`${styles.table} ${props.variant === "private" ? styles.private : ""}`}
          aria-label={props.label}
        >
          <thead>
            <tr>
              {props.columns.map((c, i) => (
                <th key={c.key ?? i} className={right(c)} scope="col">
                  {c.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {props.rows.map((row, r) => (
              <tr key={props.rowKey(row, r)}>
                {props.columns.map((c, i) => (
                  <td key={c.key ?? i} className={right(c)}>
                    {c.cell(row)}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
          {props.caption && !scroll && <caption>{props.caption}</caption>}
        </table>
      </div>
      {props.caption && scroll && <p className={styles.caption}>{props.caption}</p>}
    </>
  );
}
