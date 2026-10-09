import { ChartGrid, TimeSeriesChart } from "../components/charts/TimeSeriesChart";
import { MetricTile } from "../components/tiles/MetricTile";
import { MeasuredTile } from "../components/tiles/MeasuredTile";
import { TileGrid } from "../components/tiles/Tile";
import { Section } from "../components/ui/Section";
import { formatNumber } from "../lib/format";

export function QualitySection() {
  return (
    <Section title="Quality">
      <TileGrid>
        <MetricTile
          id="rework_pct"
          note={(m) => `target band 15–25% · score ${formatNumber(m.values.s_rework)}`}
        />
        <MetricTile
          id="test_discipline_pct"
          note={(m) =>
            `of ${formatNumber(m.n.test_discipline_pct)} commits touching prod · score ${formatNumber(m.values.s_tests)}`
          }
        />
        <MetricTile
          id="doc_discipline_pct"
          note={(m) =>
            `of ${formatNumber(m.n.doc_discipline_pct)} commits touching prod · score ${formatNumber(m.values.s_docs)}`
          }
        />
        <MeasuredTile
          input="duplication"
          id="dup_pct"
          note={(m) => `score ${formatNumber(m.values.s_dup)} · 3% → 100, 15% → 0`}
        />
      </TileGrid>
      <ChartGrid>
        <TimeSeriesChart
          title="Quality score"
          hint="geometric mean of the sub-scores present; 100 = on target"
          max={100}
          series={[
            { metric: "quality", emphasis: true },
            { metric: "s_rework", label: "Rework" },
            { metric: "s_tests", label: "Tests" },
            { metric: "s_docs", label: "Docs" },
            { metric: "s_dup", label: "Duplication" },
          ]}
        />
        <TimeSeriesChart
          title="Rework %"
          hint="lines deleted within 21 days of being added; shaded = 15–25% target band"
          unit="percent"
          band={[15, 25]}
          series={[{ metric: "rework_pct" }]}
        />
        <TimeSeriesChart
          title="Tests and docs with code %"
          hint="commits touching prod that also touch tests / docs"
          unit="percent"
          max={100}
          series={[
            { metric: "test_discipline_pct", label: "Tests" },
            { metric: "doc_discipline_pct", label: "Docs" },
          ]}
        />
      </ChartGrid>
    </Section>
  );
}
