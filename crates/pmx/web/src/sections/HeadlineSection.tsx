import { MetricTile } from "../components/tiles/MetricTile";
import { NotMeasuredTile } from "../components/tiles/NotMeasuredTile";
import { QualityTile } from "../components/tiles/ScoreTile";
import { Tile, TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { formatNumber, formatPercent } from "../lib/format";
import { useDashboard } from "../state/hooks";

/** The fleet headlines for this project (fleet-vision.md). */
export function HeadlineSection() {
  const { project } = useDashboard();
  return (
    <Section title="Headline">
      <TileGrid>
        <QualityTile />
        <MetricTile
          id="multi_stack_pct"
          note={(m) => `of ${formatNumber(m.n.multi_stack_pct)} active devs`}
        />
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
        <NotMeasuredTile id="velocity" />
        <NotMeasuredTile id="security" />
        <NotMeasuredTile id="peer_review" />
      </TileGrid>
    </Section>
  );
}
