import type { SecurityDetail, Severity } from "../../data/types";
import { formatNumber } from "../../lib/format";
import { DataTable } from "./DataTable";

const SEVERITY: Record<Severity, string> = {
  critical: "Critical",
  high: "High",
  moderate: "Moderate",
  low: "Low",
  unknown: "Unknown",
};

/** The packages with the most advisories at the latest snapshot (spec §7.1). */
export function VulnPackagesTable({ detail }: { detail: SecurityDetail }) {
  const lockfiles = Object.entries(detail.lockfiles)
    .map(([repo, files]) => `${repo}: ${files.join(", ") || "none"}`)
    .join(" · ");
  if (detail.top_packages.length === 0) {
    return (
      <p>
        No known vulnerabilities on {detail.date}. Scanned {lockfiles}.
      </p>
    );
  }
  return (
    <DataTable
      label="Most vulnerable packages"
      rows={detail.top_packages}
      rowKey={(p) => p.name}
      caption={`As of ${detail.date}. Scanned ${lockfiles}. Coverage is a lower bound where an ecosystem has no lockfile.`}
      columns={[
        { header: "Package", cell: (p) => p.name },
        { header: "Version", cell: (p) => p.version },
        { header: "Advisories", cell: (p) => formatNumber(p.count), align: "right" },
        { header: "Worst", cell: (p) => SEVERITY[p.max_severity] },
      ]}
    />
  );
}
