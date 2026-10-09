import type { AiCompare, GroupSummary } from "../../data/types";
import { formatNumber, formatPercent } from "../../lib/format";
import { DataTable, type Column } from "./DataTable";

type Row = { label: string; value(g: GroupSummary): string };

const ROWS: Row[] = [
  { label: "Commits", value: (g) => formatNumber(g.commits) },
  { label: "Lines added", value: (g) => formatNumber(g.added) },
  { label: "Median commit size", value: (g) => formatNumber(g.med_size) },
  { label: "Tests with code", value: (g) => formatPercent(g.test_pct) },
  { label: "Docs with code", value: (g) => formatPercent(g.doc_pct) },
];

/** AI-assisted vs human commits (spec §6). Project level only, never per developer. */
export function AiCompareTable({ compare }: { compare: AiCompare }) {
  const columns: Column<Row>[] = [
    { header: `${compare.from} – ${compare.to}`, cell: (r) => r.label },
    { header: "AI-assisted", cell: (r) => r.value(compare.ai), align: "right" },
    { header: "Human", cell: (r) => r.value(compare.human), align: "right" },
  ];
  return (
    <DataTable
      label="AI-assisted vs human"
      columns={columns}
      rows={ROWS}
      rowKey={(r) => r.label}
      caption="Project level only, never per developer (spec §6)."
    />
  );
}
