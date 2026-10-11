import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { PackagesTable } from "../components/tables/PackagesTable";
import { PointsTable } from "../components/tables/PointsTable";
import { MetricTile } from "../components/tiles/MetricTile";
import { NotMeasuredTile } from "../components/tiles/NotMeasuredTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { bucketMetrics } from "../data/project";
import { formatDate } from "../lib/dates";
import { formatNumber } from "../lib/format";
import { useDashboard, useMeasured } from "../state/hooks";
import { velocityNote } from "../metrics/velocity";

/** Velocity: six-axis complexity points of delivered work packages per FTE-day (spec §4). */
export function VelocitySection() {
  const { project, leads, range } = useDashboard();
  const measured = useMeasured();
  const v = project.velocity;
  if (!v || !measured("velocity")) {
    return (
      <Section title="Velocity">
        <TileGrid>
          <NotMeasuredTile id="velocity" />
        </TileGrid>
      </Section>
    );
  }
  const [start, end] = [formatDate(range.start), formatDate(range.end)];
  const inRange = v.packages.filter((p) => p.date >= start && p.date <= end);
  const unscored = inRange.filter((p) => p.points == null).length;
  const escalated = inRange.filter((p) => p.escalated).length;
  const unsure = inRange.filter((p) => p.low_confidence).length;
  const label = bucketMetrics(project, range)?.label;
  const points = leads && label ? leads.points[label] : undefined;
  return (
    <Section title="Velocity">
      <p>
        Work packages (a ticket, else one person&apos;s untracked week) scored on the {v.rubric} rubric by{" "}
        {v.classifiers.join(", ") || "no classifier yet"}; scored from {v.options.since}.
        {v.classifiers.length > 1 ? " Scores from different classifiers are not strictly comparable." : ""}
        {unscored > 0
          ? ` ${formatNumber(unscored)} package(s) in this range are not scored yet, so the range has no Velocity.`
          : ""}
        {escalated > 0
          ? ` ${formatNumber(escalated)} went to the fallback classifier after unsure answers.`
          : ""}
        {unsure > 0 ? ` ${formatNumber(unsure)} stayed below the confidence gate.` : ""}
        {v.models.length > v.classifiers.length
          ? ` Answered by ${v.models.join(", ")}: a declined request ran on a fallback model.`
          : ""}
      </p>
      <TileGrid>
        <MetricTile id="velocity" note={velocityNote} />
        <MetricTile id="velocity_points" />
        <MetricTile id="points_per_commit" />
        <MetricTile id="points_per_kloc" />
        <MetricTile id="verification_debt_pts" />
      </TileGrid>
      <ChartGrid>
        <TimeSeriesChart
          title="Velocity"
          hint="points per FTE-day (per active engineer-day when FTE is unknown); read beside Quality"
          decimals={2}
          series={[
            { metric: "velocity" },
            { metric: "velocity_points", label: "Points", axis: 1, kind: "bar", color: 6 },
          ]}
        />
        <TimeSeriesChart
          title="Complexity density"
          hint="points per 1,000 added lines and per commit"
          decimals={2}
          series={[
            { metric: "points_per_kloc", label: "Points / KLOC" },
            { metric: "points_per_commit", label: "Points / commit", axis: 1 },
          ]}
        />
      </ChartGrid>
      <PackagesTable packages={inRange.slice(0, 25)} total={inRange.length} />
      {points && <PointsTable points={points} />}
    </Section>
  );
}
