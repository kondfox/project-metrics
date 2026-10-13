import { useEffect, useRef, type ReactNode } from "react";
import styles from "./layout.module.css";

/**
 * Keeps what's being viewed (the project or fleet, cadence and range) in sight while scrolling. Its
 * height is published as `--sticky-top`, so a section scrolled to lands below it.
 */
export function StickyTop({ children }: { children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || typeof ResizeObserver === "undefined") return;
    const root = document.documentElement.style;
    const ro = new ResizeObserver(() => root.setProperty("--sticky-top", `${el.offsetHeight}px`));
    ro.observe(el);
    return () => {
      ro.disconnect();
      root.removeProperty("--sticky-top");
    };
  }, []);
  return (
    <div ref={ref} className={styles.sticky}>
      {children}
    </div>
  );
}
