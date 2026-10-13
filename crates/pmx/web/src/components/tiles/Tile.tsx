import { useContext, type ReactNode } from "react";
import { Badge } from "../ui/Badge";
import { HelpTip } from "../ui/HelpTip";
import { specUrl } from "../../lib/docs";
import { scrollToSection, SectionLinkContext } from "./sectionLink";
import styles from "./tiles.module.css";

export type TileState = "value" | "empty" | "notMeasured";

export interface TileProps {
  label: string;
  state?: TileState;
  value?: ReactNode;
  note?: ReactNode;
  /** Explains the tile behind a "?" next to its label. */
  help?: ReactNode;
  /** The metrics-spec section that defines it (e.g. "§3.1"): linked from the tile's corner. */
  doc?: string;
  /** Denominator, shown as a badge when it is below the low-n threshold. */
  lowN?: { n: number; threshold: number } | null;
}

/**
 * One number with its context. Three states, never confused: a value, "no data" (no denominator in
 * this range: spec §1.1, no data ≠ zero), and "not measured" (the input isn't collected yet).
 */
export function Tile({ label, state = "value", value, note, help, doc, lowN }: TileProps) {
  const section = useContext(SectionLinkContext);
  const docUrl = doc ? specUrl(doc) : null;
  const cls = [styles.tile, state !== "value" && styles[state], lowN && styles.lowN, section && styles.linked]
    .filter(Boolean)
    .join(" ");
  return (
    <div className={cls} role="group" aria-label={label}>
      {docUrl && (
        <a
          className={styles.doc}
          href={docUrl}
          target="_blank"
          rel="noopener noreferrer"
          aria-label={`How ${label} is measured`}
          title={`How it's measured: metrics spec ${doc}`}
        >
          ↗
        </a>
      )}
      <div className={styles.label}>
        {section ? (
          <button
            type="button"
            className={styles.jump}
            title={`go to the ${label} section`}
            onClick={() => scrollToSection(section)}
          >
            {label}
          </button>
        ) : (
          label
        )}
        {help && <HelpTip>{help}</HelpTip>}
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

/** Makes the tiles inside it open a section of the page: the whole tile is clickable. */
export function SectionLink({ to, children }: { to: string; children: ReactNode }) {
  return <SectionLinkContext.Provider value={to}>{children}</SectionLinkContext.Provider>;
}
