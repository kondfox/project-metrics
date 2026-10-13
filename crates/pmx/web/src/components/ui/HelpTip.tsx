import { useId, useState, type ReactNode } from "react";
import styles from "./ui.module.css";

/**
 * A "?" that explains a heading on hover or keyboard focus. The text is placed against the window
 * (`position: fixed`), so a table's scroll box doesn't clip it.
 */
export function HelpTip({ children }: { children: ReactNode }) {
  const id = useId();
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  const show = (e: { currentTarget: HTMLElement }) => {
    const r = e.currentTarget.getBoundingClientRect();
    setAt({ x: Math.min(r.left, window.innerWidth - 336), y: r.bottom + 6 });
  };
  const hide = () => setAt(null);
  return (
    <>
      <span
        className={styles.help}
        tabIndex={0}
        role="button"
        aria-label="what this means"
        aria-describedby={id}
        onMouseEnter={show}
        onMouseLeave={hide}
        onFocus={show}
        onBlur={hide}
      >
        ?
      </span>
      <span
        id={id}
        role="tooltip"
        className={styles.tip}
        hidden={!at}
        style={at ? { left: Math.max(8, at.x), top: at.y } : undefined}
      >
        {children}
      </span>
    </>
  );
}
