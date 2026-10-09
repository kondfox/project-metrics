import type { ReactNode } from "react";
import styles from "./ui.module.css";

export function Badge({ children, title }: { children: ReactNode; title?: string }) {
  return (
    <span className={styles.badge} title={title}>
      {children}
    </span>
  );
}
