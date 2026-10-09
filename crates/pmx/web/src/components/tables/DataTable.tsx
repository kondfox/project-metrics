import type { ReactNode } from "react";
import styles from "./tables.module.css";

export interface Column<T> {
  header: ReactNode;
  cell(row: T): ReactNode;
  align?: "left" | "right";
  key?: string;
}

/** A plain, typed table. `private` marks lead-only content. */
export function DataTable<T>(props: {
  columns: ReadonlyArray<Column<T>>;
  rows: ReadonlyArray<T>;
  rowKey(row: T, index: number): string;
  caption?: ReactNode;
  label?: string;
  variant?: "default" | "private";
}) {
  const right = (c: Column<T>) => (c.align === "right" ? styles.right : undefined);
  return (
    <div className={styles.wrap}>
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
        {props.caption && <caption>{props.caption}</caption>}
      </table>
    </div>
  );
}
