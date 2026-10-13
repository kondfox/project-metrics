import { useEffect, useState } from "react";

/** Chart colors, read from the design tokens in theme.css. */
export interface ChartTheme {
  ink: string;
  muted: string;
  line: string;
  faint: string;
  good: string;
  series: string[];
}

export function readChartTheme(el: Element = document.documentElement): ChartTheme {
  const css = getComputedStyle(el);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  return {
    ink: v("--ink", "#1d1d1b"),
    muted: v("--muted", "#6b6b66"),
    line: v("--line", "#e4e3df"),
    faint: v("--faint", "#f0efeb"),
    good: v("--good", "#1f8a5b"),
    series: [1, 2, 3, 4, 5, 6, 7].map((i) => v(`--series-${i}`, "#888888")),
  };
}

/** The theme, refreshed when the OS or the viewer (`data-theme`, ThemeToggle) switches modes. */
export function useChartTheme(): ChartTheme {
  const [theme, setTheme] = useState(readChartTheme);
  useEffect(() => {
    const update = () => setTheme(readChartTheme());
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    mq?.addEventListener("change", update);
    const mo = new MutationObserver(update);
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => {
      mq?.removeEventListener("change", update);
      mo.disconnect();
    };
  }, []);
  return theme;
}
