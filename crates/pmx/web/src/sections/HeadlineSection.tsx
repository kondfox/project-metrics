import { MeasuredTile } from "../components/tiles/MeasuredTile";
import { MetricTile } from "../components/tiles/MetricTile";
import { QualityTile, SecurityTile } from "../components/tiles/ScoreTile";
import { SectionLink, Tile, TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { formatNumber, formatPercent } from "../lib/format";
import { useDashboard } from "../state/hooks";
import { velocityNote } from "../metrics/velocity";

/** The fleet headlines for this project (fleet-vision.md), in the fleet's column order. */
export function HeadlineSection() {
  const { project } = useDashboard();
  return (
    <Section title="Headline">
      <TileGrid>
        <SectionLink to="velocity">
          <MeasuredTile input="velocity" id="velocity" note={velocityNote} />
        </SectionLink>
        <SectionLink to="quality">
          <QualityTile />
        </SectionLink>
        <SectionLink to="security">
          <SecurityTile />
        </SectionLink>
        <SectionLink to="multi-stack">
          <MetricTile
            id="multi_stack_pct"
            note={(m) => `of ${formatNumber(m.n.multi_stack_pct)} active devs`}
          />
        </SectionLink>
        <SectionLink to="ai">
          {project.project.ai_attribution ? (
            <MetricTile
              id="ai_assist_commit_pct"
              note={(m) => `${formatPercent(m.values.ai_assist_line_pct)} of added lines`}
            />
          ) : (
            <Tile
              label="AI-assisted"
              state="notMeasured"
              note="not tracked: AI trailers are stripped by policy"
            />
          )}
        </SectionLink>
        <SectionLink to="peer-review">
          <MeasuredTile
            input="peer_review"
            id="review_coverage_pct"
            note={(m) => `of ${formatNumber(m.values.pr_count)} merged PRs`}
          />
        </SectionLink>
      </TileGrid>
    </Section>
  );
}
