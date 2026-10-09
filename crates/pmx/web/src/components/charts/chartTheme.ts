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

/** The theme, refreshed when the OS switches between light and dark. */
export function useChartTheme(): ChartTheme {
  const [theme, setTheme] = useState(readChartTheme);
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!mq) return;
    const update = () => setTheme(readChartTheme());
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);
  return theme;
}
