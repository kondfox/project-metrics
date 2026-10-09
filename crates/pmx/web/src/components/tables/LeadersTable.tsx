import type { Leader } from "../../data/types";
import { formatNumber } from "../../lib/format";
import { roleLabel } from "../../metrics/registry";
import { DataTable } from "./DataTable";

/** Per-person multi-stack line-up: the lead's private view only (plan §3.5). */
export function LeadersTable({ leaders }: { leaders: Leader[] }) {
  if (leaders.length === 0) return <p>No one added lines in a breadth role in this range.</p>;
  return (
    <DataTable
      label="Multi-stack leaders"
      variant="private"
      rows={leaders}
      rowKey={(l) => l.name}
      caption="Private: only the lead's own copy (out/private) shows people."
      columns={[
        { header: "Person", cell: (l) => l.name },
        { header: "Stacks ≥ 40 lines", cell: (l) => l.stacks.map(roleLabel).join(", ") || "–" },
        { header: "Technologies", cell: (l) => l.techs.join(", ") || "–" },
        { header: "Breadth", cell: (l) => formatNumber(l.breadth * 100), align: "right" },
        { header: "Lines", cell: (l) => formatNumber(l.lines), align: "right" },
      ]}
    />
  );
}
