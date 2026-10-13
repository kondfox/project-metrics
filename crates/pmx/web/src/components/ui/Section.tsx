import type { ReactNode } from "react";
import { sectionElementId } from "../tiles/sectionLink";
import { ErrorBoundary } from "./ErrorBoundary";
import styles from "./ui.module.css";

/**
 * A titled block; a failure inside it is contained (the rest of the dashboard still renders). An `id`
 * lets a headline tile link to it.
 */
export function Section({ title, id, children }: { title: string; id?: string; children: ReactNode }) {
  return (
    <section className={styles.section} aria-label={title} id={id && sectionElementId(id)}>
      <h2>{title}</h2>
      <ErrorBoundary>
        <div className={styles.stack}>{children}</div>
      </ErrorBoundary>
    </section>
  );
}
