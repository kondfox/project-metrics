import type { Leader } from "../../data/types";
import { formatNumber } from "../../lib/format";
import { roleLabel } from "../../metrics/registry";
import { HelpTip } from "../ui/HelpTip";
import { RoleBadge } from "../ui/RoleBadge";
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
      columns={[
        { header: "Person", cell: (l) => l.name },
        {
          header: "Stacks ≥ 40 lines",
          cell: (l) =>
            l.stacks.length
              ? [...l.stacks]
                  .sort((a, b) => roleLabel(a).localeCompare(roleLabel(b)))
                  .map((r) => <RoleBadge key={r} role={r} />)
              : "–",
        },
        { header: "Technologies", cell: (l) => l.techs.join(", ") || "–" },
        {
          header: (
            <>
              Breadth
              <HelpTip>
                How evenly the person&apos;s added lines spread over their stacks (those with ≥ 40 lines),
                0–100. 0: one stack only. 100: the same number of lines in each of their stacks. A 90 / 10
                split between two stacks is 47. It measures how even, not how many: the Stacks column shows
                how many. (Normalized entropy of their lines per stack; the Breadth index above is its mean
                over people.)
              </HelpTip>
            </>
          ),
          cell: (l) => formatNumber(l.breadth * 100),
          align: "right",
        },
      ]}
    />
  );
}
