import { band } from "../../lib/format";
import styles from "./ui.module.css";

/** A–E band of a 0–100 score (spec §1.7). */
export function BandBadge({ score }: { score: number }) {
  const b = band(score);
  return (
    <span className={`${styles.band} ${styles[b]}`} aria-label={`band ${b}`}>
      {b}
    </span>
  );
}
