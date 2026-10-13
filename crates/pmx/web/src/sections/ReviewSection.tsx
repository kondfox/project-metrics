import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { MeasuredTile } from "../components/tiles/MeasuredTile";
import { MetricTile } from "../components/tiles/MetricTile";
import { NotMeasuredTile } from "../components/tiles/NotMeasuredTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { useDashboard, useMeasured } from "../state/hooks";

const HOSTS: Record<string, string> = { github: "GitHub", gitlab: "GitLab" };

/** Pull-request flow from the code host: peer review, cycle time, review wait, mentoring (spec §8). */
export function ReviewSection() {
  const pulls = useDashboard().project.meta.pull_requests;
  const measured = useMeasured();
  if (!pulls || !measured("peer_review")) {
    return (
      <Section title="Peer review" id="peer-review">
        <TileGrid>
          <NotMeasuredTile id="peer_review" />
        </TileGrid>
      </Section>
    );
  }
  const host = HOSTS[pulls.host] ?? pulls.host;
  const repos = pulls.repos.length === 1 ? pulls.repos[0] : `${pulls.repos.length} repos`;
  return (
    <Section title="Peer review" id="peer-review">
      <p>
        Merged {pulls.host === "gitlab" ? "merge requests" : "pull requests"} from {host} ({repos}), by merge
        date.
      </p>
      <TileGrid>
        <MetricTile id="review_coverage_pct" />
        <MetricTile id="pr_count" />
        <MetricTile id="pr_cycle_median_h" />
        <MetricTile id="review_wait_median_h" />
        <MeasuredTile input="pr_size" id="pr_size_median" />
        <MetricTile id="mentoring_pairs" />
      </TileGrid>
      <ChartGrid>
        <TimeSeriesChart
          title="Peer review %"
          hint="merged PRs reviewed by someone other than the author"
          unit="percent"
          max={100}
          series={[
            { metric: "review_coverage_pct" },
            { metric: "pr_count", label: "PRs merged", axis: 1, kind: "bar", color: 6 },
          ]}
        />
        <TimeSeriesChart
          title="Cycle time and review wait"
          hint="median hours"
          decimals={1}
          series={[
            { metric: "pr_cycle_median_h", label: "Cycle time" },
            { metric: "review_wait_median_h", label: "Review wait" },
          ]}
        />
        <TimeSeriesChart
          title="Mentoring pairs"
          hint="an expert in the stack (≥ 3 PRs and ≥ 25% of its top author's, over the past year) reviewed a non-expert"
          unit="count"
          series={[{ metric: "mentoring_pairs", kind: "bar" }]}
        />
      </ChartGrid>
    </Section>
  );
}
