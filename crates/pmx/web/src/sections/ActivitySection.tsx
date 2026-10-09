import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { MetricTile } from "../components/tiles/MetricTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { mixRoles } from "../data/project";
import type { Role } from "../data/types";
import { formatNumber } from "../lib/format";
import { roleLabel } from "../metrics/registry";
import { useDashboard } from "../state/hooks";

/** Context only: never headline targets (synthesis.md §8). */
export function ActivitySection() {
  const roles = mixRoles(useDashboard().project) as Role[];
  return (
    <Section title="Activity">
      <TileGrid>
        <MetricTile id="commits" />
        <MetricTile id="added" />
        <MetricTile id="commit_med" note={(m) => `median lines · p90 ${formatNumber(m.values.commit_p90)}`} />
        <MetricTile
          id="commits_per_dev_med"
          note={(m) => `median of ${formatNumber(m.n.commits_per_dev_med)} people`}
        />
        <MetricTile id="active_devs" />
      </TileGrid>
      <ChartGrid>
        <TimeSeriesChart
          title="Commits and lines added"
          series={[
            { metric: "commits", kind: "bar" },
            { metric: "added", axis: 1 },
          ]}
        />
        <TimeSeriesChart
          title="Commit size (lines)"
          hint="added + deleted source lines per commit"
          series={[
            { metric: "commit_med", label: "Median" },
            { metric: "commit_p90", label: "p90" },
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
          }))}
        />
      </ChartGrid>
    </Section>
  );
}
