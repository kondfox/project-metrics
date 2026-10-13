import type { CSSProperties } from "react";
import type { Role } from "../../data/types";
import { roleColor, roleLabel } from "../../metrics/registry";
import styles from "./ui.module.css";

/** A stack role in its own color, the same as in the Stack mix chart. */
export function RoleBadge({ role }: { role: Role }) {
  const style = { "--role": `var(--series-${roleColor(role) + 1})` } as CSSProperties;
  return (
    <span className={styles.role} style={style}>
      {roleLabel(role)}
    </span>
  );
}
