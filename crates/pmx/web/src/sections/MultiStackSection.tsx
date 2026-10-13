import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { LeadersTable } from "../components/tables/LeadersTable";
import { MetricTile } from "../components/tiles/MetricTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { mixRoles } from "../data/project";
import type { Role } from "../data/types";
import { roleColor, roleLabel } from "../metrics/registry";
import { useDashboard, useRangeMetrics } from "../state/hooks";

export function MultiStackSection() {
  const leaders = useRangeMetrics()?.leaders;
  const roles = mixRoles(useDashboard().project) as Role[];
  return (
    <Section title="Multi-stack" id="multi-stack">
      <TileGrid>
        <MetricTile id="multi_stack_pct" />
        <MetricTile
          id="breadth_index"
          help={
            <>
              The mean of everyone&apos;s Breadth (in the table below): how evenly each active
              developer&apos;s added lines spread over their stacks (those with ≥ 40 lines), 0–100. 0:
              everyone works in one stack. 100: everyone splits their lines equally across their stacks.
              Someone with a single stack counts as 0, so the index falls when single-stack people join, even
              if no one narrows.
            </>
          }
        />
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
        <TimeSeriesChart
          title="Stack mix %"
          hint="share of added source lines by role"
          unit="percent"
          max={100}
          series={roles.map((r) => ({
            metric: `mix_${r}` as const,
            label: roleLabel(r),
            kind: "bar",
            stack: "mix",
            color: roleColor(r),
          }))}
        />
      </ChartGrid>
      {leaders && <LeadersTable leaders={leaders} />}
    </Section>
  );
}
