import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { HotspotsTable } from "../components/tables/HotspotsTable";
import { MeasuredTile } from "../components/tiles/MeasuredTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { useDashboard, useMeasured } from "../state/hooks";

/** Duplication, complexity and hotspots: snapshot metrics of the tree (spec §3.4–3.5). */
export function CodeHealthSection() {
  const { project } = useDashboard();
  const measured = useMeasured();
  return (
    <Section title="Code health">
      <TileGrid>
        <MeasuredTile input="complexity" id="cx_per_kloc" />
        <MeasuredTile input="complexity" id="kloc" />
      </TileGrid>
      <ChartGrid>
        {measured("duplication") && (
          <TimeSeriesChart
            title="Duplication %"
            hint="duplicated lines at each snapshot; Quality's 4th constituent"
            unit="percent"
            decimals={2}
            series={[{ metric: "dup_pct" }]}
          />
        )}
        {measured("complexity") && (
          <TimeSeriesChart
            title="Complexity / KLOC"
            hint="within-project trend only: complexity depends on the stack"
            decimals={1}
            series={[
              { metric: "cx_per_kloc" },
              { metric: "kloc", label: "KLOC", axis: 1, kind: "bar", color: 6 },
            ]}
          />
        )}
      </ChartGrid>
      {project.hotspots.map((r) => (
        <HotspotsTable key={r.repo} repo={r} />
      ))}
    </Section>
  );
}
