import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { AiCompareTable } from "../components/tables/AiCompareTable";
import { Section } from "../components/ui/Section";
import { useDashboard } from "../state/hooks";

export function AiSection() {
  const { project } = useDashboard();
  if (!project.project.ai_attribution) {
    return (
      <Section title="AI-assisted">
        <p>Not tracked for this project: AI trailers are stripped by policy (spec §6).</p>
      </Section>
    );
  }
  return (
    <Section title="AI-assisted vs human (trailing 12 months)">
      {project.ai_compare ? <AiCompareTable compare={project.ai_compare} /> : <p>No commits to compare.</p>}
      <ChartGrid>
        <TimeSeriesChart
          title="AI-assisted %"
          hint="Co-Authored-By: Claude trailer"
          unit="percent"
          max={100}
          series={[
            { metric: "ai_assist_commit_pct", label: "Commits" },
            { metric: "ai_assist_line_pct", label: "Lines" },
          ]}
        />
      </ChartGrid>
    </Section>
  );
}
