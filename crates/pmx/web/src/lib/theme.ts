import { useCallback, useEffect, useState } from "react";

export type Theme = "light" | "dark";

const KEY = "pmx.theme";
const darkQuery = () => window.matchMedia?.("(prefers-color-scheme: dark)");

/** The viewer's own choice, if they made one (browser storage can be blocked: then none). */
export function storedTheme(): Theme | null {
  try {
    const t = localStorage.getItem(KEY);
    return t === "light" || t === "dark" ? t : null;
  } catch {
    return null;
  }
}

const systemTheme = (): Theme => (darkQuery()?.matches ? "dark" : "light");

/** `data-theme` on <html> overrides the system's mode (theme.css); none follows the system. */
export function applyTheme(t: Theme | null) {
  if (t) document.documentElement.dataset.theme = t;
  else delete document.documentElement.dataset.theme;
}

/** The mode shown and a switch; the choice is remembered, else the page follows the system. */
export function useTheme(): [Theme, () => void] {
  const [chosen, setChosen] = useState(storedTheme);
  const [system, setSystem] = useState(systemTheme);
  useEffect(() => {
    const mq = darkQuery();
    if (!mq) return;
    const update = () => setSystem(systemTheme());
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);
  const theme = chosen ?? system;
  const toggle = useCallback(() => {
    const next: Theme = theme === "dark" ? "light" : "dark";
    try {
      localStorage.setItem(KEY, next);
    } catch {
      // Not remembered; it still applies to this page.
    }
    applyTheme(next);
    setChosen(next);
  }, [theme]);
  return [theme, toggle];
}
