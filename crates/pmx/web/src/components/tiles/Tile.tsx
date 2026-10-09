import type { ReactNode } from "react";
import { Badge } from "../ui/Badge";
import styles from "./tiles.module.css";

export type TileState = "value" | "empty" | "notMeasured";

export interface TileProps {
  label: string;
  state?: TileState;
  value?: ReactNode;
  note?: ReactNode;
  /** Denominator, shown as a badge when it is below the low-n threshold. */
  lowN?: { n: number; threshold: number } | null;
}

/**
 * One number with its context. Three states, never confused: a value, "no data" (no denominator in
 * this range: spec §1.1, no data ≠ zero), and "not measured" (the input isn't collected yet).
 */
export function Tile({ label, state = "value", value, note, lowN }: TileProps) {
  const cls = [styles.tile, state !== "value" && styles[state], lowN && styles.lowN]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls} role="group" aria-label={label}>
      <div className={styles.label}>
        {label}
        {lowN && (
          <Badge title={`fewer than ${lowN.threshold} events: read with care`}>low n · {lowN.n}</Badge>
        )}
      </div>
      <div className={styles.value}>
        {state === "value" ? value : state === "empty" ? "no data" : "not measured"}
      </div>
      {note && <div className={styles.note}>{note}</div>}
    </div>
  );
}

export function TileGrid({ children }: { children: ReactNode }) {
  return <div className={styles.grid}>{children}</div>;
}
