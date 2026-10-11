import { formatNumber } from "../../lib/format";
import { DataTable } from "./DataTable";

/** Velocity points per person: the lead's private view only, never a headline (spec §4). */
export function PointsTable({ points }: { points: Record<string, number> }) {
  const rows = Object.entries(points)
    .map(([name, pts]) => ({ name, pts }))
    .sort((a, b) => b.pts - a.pts || a.name.localeCompare(b.name));
  if (rows.length === 0) return null;
  return (
    <DataTable
      label="Points by person"
      variant="private"
      rows={rows}
      rowKey={(r) => r.name}
      caption="Private: each package's points split by its commits. Read with the work behind them, never as a ranking."
      columns={[
        { header: "Person", cell: (r) => r.name },
        { header: "Points", cell: (r) => formatNumber(r.pts, 1), align: "right" },
      ]}
    />
  );
}
