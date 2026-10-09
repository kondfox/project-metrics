import { BarChart, LineChart } from "echarts/charts";
import { GridComponent, LegendComponent, MarkAreaComponent, TooltipComponent } from "echarts/components";
import * as echarts from "echarts/core";
import type { EChartsCoreOption } from "echarts/core";
import { SVGRenderer } from "echarts/renderers";
import { useEffect, useRef } from "react";

// Only what the dashboard uses, so the page stays small. Register new chart types here.
echarts.use([
  LineChart,
  BarChart,
  GridComponent,
  LegendComponent,
  MarkAreaComponent,
  TooltipComponent,
  SVGRenderer,
]);

/** Thin lifecycle wrapper: init on mount, replace the option on change, resize with the box. */
export function EChart({ option, height = 240 }: { option: EChartsCoreOption; height?: number }) {
  const el = useRef<HTMLDivElement>(null);
  const chart = useRef<echarts.ECharts | null>(null);

  useEffect(() => {
    if (!el.current) return;
    const c = echarts.init(el.current, null, { renderer: "svg" });
    chart.current = c;
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => c.resize());
    ro?.observe(el.current);
    return () => {
      ro?.disconnect();
      c.dispose();
      chart.current = null;
    };
  }, []);

  useEffect(() => {
    chart.current?.setOption(option, { notMerge: true });
  }, [option]);

  return <div ref={el} style={{ height }} />;
}
