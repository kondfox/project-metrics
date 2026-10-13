import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { PackagesTable } from "../components/tables/PackagesTable";
import { PointsTable } from "../components/tables/PointsTable";
import { MetricTile } from "../components/tiles/MetricTile";
import { NotMeasuredTile } from "../components/tiles/NotMeasuredTile";
import { Tile, TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { bucketMetrics } from "../data/project";
import { formatDate } from "../lib/dates";
import { formatNumber } from "../lib/format";
import { useDashboard, useMeasured } from "../state/hooks";
import { fteOver, velocityNote } from "../metrics/velocity";
import type { DateRange } from "../lib/dates";
import type { VelocityDetail, VelocityOptions } from "../data/types";

/** How the packages were grouped (spec §4), and where PR data was missing. */
function grouping(h: VelocityDetail["health"]): string {
  const total = h.by_ticket + h.by_pr + h.by_author_week;
  if (total === 0) return "";
  const parts = ` Grouped: ${formatNumber(h.by_ticket)} by ticket, ${formatNumber(h.by_pr)} by PR / MR, ${formatNumber(h.by_author_week)} by author-week.`;
  const missing =
    h.no_pr_data.length > 0
      ? ` No PR data for ${h.no_pr_data.join(", ")}: their untagged commits are grouped by author-week, which is coarser.`
      : "";
  return parts + missing;
}

/** The capacity Velocity is divided by: mean FTE over the range's business days (spec §10.2). */
function FteTile({ options, range }: { options: VelocityOptions; range: DateRange }) {
  const f = fteOver(options, range);
  if (options.fte.length === 0)
    return (
      <Tile
        label="FTE"
        state="notMeasured"
        note="FTE unknown ([fte] in pmx.toml): Velocity uses active engineer-days"
      />
    );
  if (!f)
    return (
      <Tile
        label="FTE"
        state="empty"
        note="no FTE for some business days: Velocity uses active engineer-days"
      />
    );
  return (
    <Tile
      label="FTE"
      value={formatNumber(f.mean, 1)}
      note={`${formatNumber(f.fteDays, 1)} FTE-days over ${formatNumber(f.businessDays)} business days`}
    />
  );
}

/** Velocity: six-axis complexity points of delivered work packages per FTE-day (spec §4). */
export function VelocitySection() {
  const { project, leads, range } = useDashboard();
  const measured = useMeasured();
  const v = project.velocity;
  if (!v || !measured("velocity")) {
    return (
      <Section title="Velocity" id="velocity">
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
    <Section title="Velocity" id="velocity">
      <p>
        Work packages (a ticket, else a PR / MR, else one person&apos;s untracked week) scored on the{" "}
        {v.rubric} rubric by {v.classifiers.join(", ") || "no classifier yet"}; scored from {v.options.since}.
        {grouping(v.health)}
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
        <FteTile options={v.options} range={range} />
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
      <PackagesTable packages={inRange} />
      {points && <PointsTable points={points} />}
    </Section>
  );
}
