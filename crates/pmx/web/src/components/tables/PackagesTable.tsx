import type { Package } from "../../data/types";
import { formatNumber } from "../../lib/format";
import { DataTable } from "./DataTable";

const AXES = ["R1", "R2", "R3", "R4", "R5", "R6"] as const;

/** The scored work packages of a range, newest first (spec §4, §10.3): all of them, in a box that
 * scrolls and shows ten at a time. */
export function PackagesTable({ packages }: { packages: Package[] }) {
  if (packages.length === 0) return null;
  return (
    <DataTable
      label="Work packages"
      rows={packages}
      rowKey={(p) => `${p.id} ${p.date}`}
      visibleRows={10}
      caption={`${formatNumber(packages.length)} packages in this range, newest first. R1 layers · R2 data model · R3 contract · R4 algorithmic · R5 blast radius · R6 verification need.`}
      columns={[
        { header: "Date", cell: (p) => p.date, nowrap: true },
        { header: "Package", cell: (p) => p.description || p.id },
        ...AXES.map((a, i) => ({
          header: a,
          cell: (p: Package) => (p.axes ? String(p.axes[i]) : "–"),
          align: "right" as const,
        })),
        { header: "Size", cell: (p) => p.size ?? "not scored" },
        { header: "Points", cell: (p) => (p.points == null ? "–" : formatNumber(p.points)), align: "right" },
        { header: "Tests", cell: (p) => (p.tests ? "yes" : "no") },
      ]}
    />
  );
}
