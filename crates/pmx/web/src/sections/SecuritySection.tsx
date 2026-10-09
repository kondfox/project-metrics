import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { VulnPackagesTable } from "../components/tables/VulnPackagesTable";
import { MeasuredTile } from "../components/tiles/MeasuredTile";
import { SecurityTile } from "../components/tiles/ScoreTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { formatNumber } from "../lib/format";
import { useDashboard, useMeasured } from "../state/hooks";

/** Dependency vulnerabilities, live secrets and own-code SAST (spec §7). */
export function SecuritySection() {
  const { project } = useDashboard();
  const measured = useMeasured();
  return (
    <Section title="Security">
      <TileGrid>
        <SecurityTile />
        <MeasuredTile
          input="dependency_vulnerabilities"
          id="vuln_critical"
          label="Critical advisories"
          note={(m) =>
            `${formatNumber(m.values.vuln_advisories)} advisories in ${formatNumber(m.values.vuln_packages)} packages`
          }
        />
        <MeasuredTile
          input="secrets"
          id="secrets_high"
          note={() => "the Security gate: one caps the score at 40"}
        />
        <MeasuredTile
          input="sast"
          id="sast_high"
          note={(m) =>
            `${formatNumber(m.values.sast_medium)} medium · ${formatNumber(m.values.sast_low)} low · ${formatNumber(m.values.sast_per_kloc, 3)} per KLOC`
          }
        />
        <MeasuredTile input="sast" id="suppressions" />
      </TileGrid>
      <ChartGrid>
        {measured("dependency_vulnerabilities") && (
          <TimeSeriesChart
            title="Dependency advisories"
            hint="open advisories by severity at each snapshot (today's vulnerability database)"
            series={[
              { metric: "vuln_critical", label: "Critical", kind: "bar", stack: "v", color: 4 },
              { metric: "vuln_high", label: "High", kind: "bar", stack: "v", color: 1 },
              { metric: "vuln_moderate", label: "Moderate", kind: "bar", stack: "v", color: 6 },
              { metric: "vuln_low", label: "Low", kind: "bar", stack: "v", color: 5 },
            ]}
          />
        )}
        {measured("sast") && (
          <TimeSeriesChart
            title="Own-code SAST findings"
            hint="outside tests, by severity; trend only, never ranked across projects"
            series={[
              { metric: "sast_high", label: "High", kind: "bar", stack: "s", color: 4 },
              { metric: "sast_medium", label: "Medium", kind: "bar", stack: "s", color: 1 },
              { metric: "sast_low", label: "Low", kind: "bar", stack: "s", color: 6 },
              { metric: "sast_new", label: "New", color: 0 },
              { metric: "sast_fixed", label: "Fixed", color: 2 },
            ]}
          />
        )}
        {measured("secrets") && (
          <TimeSeriesChart
            title="Secrets in the tree"
            hint="live (high confidence, untriaged) and low-confidence hits; values are never stored"
            series={[
              { metric: "secrets_high", label: "Live", color: 4 },
              { metric: "secrets_low", label: "Low confidence", color: 6 },
            ]}
          />
        )}
      </ChartGrid>
      {project.security && <VulnPackagesTable detail={project.security} />}
    </Section>
  );
}
