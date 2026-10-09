import type { ReactNode } from "react";
import { ErrorBoundary } from "./ErrorBoundary";
import styles from "./ui.module.css";

/** A titled block; a failure inside it is contained (the rest of the dashboard still renders). */
export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className={styles.section} aria-label={title}>
      <h2>{title}</h2>
      <ErrorBoundary>
        <div className={styles.stack}>{children}</div>
      </ErrorBoundary>
    </section>
  );
}
