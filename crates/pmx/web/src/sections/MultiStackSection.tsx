import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { LeadersTable } from "../components/tables/LeadersTable";
import { MetricTile } from "../components/tiles/MetricTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { useRangeMetrics } from "../state/hooks";

export function MultiStackSection() {
  const leaders = useRangeMetrics()?.leaders;
  return (
    <Section title="Multi-stack">
      <TileGrid>
        <MetricTile id="multi_stack_pct" />
        <MetricTile id="breadth_index" />
        <MetricTile id="techs_per_dev" />
      </TileGrid>
      <ChartGrid>
        <TimeSeriesChart
          title="Multi-stack devs and active devs"
          unit="percent"
          series={[
            { metric: "multi_stack_pct", label: "Multi-stack %" },
            { metric: "breadth_index", color: 2 },
            { metric: "active_devs", kind: "bar", axis: 1, color: 6 },
          ]}
        />
      </ChartGrid>
      {leaders && <LeadersTable leaders={leaders} />}
    </Section>
  );
}
