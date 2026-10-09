import type { RepoHotspots } from "../../data/types";
import { formatNumber } from "../../lib/format";
import { DataTable } from "./DataTable";

/** Churn × complexity per file (spec §3.5); ranked within each repo only. */
export function HotspotsTable({ repo }: { repo: RepoHotspots }) {
  if (repo.files.length === 0) return null;
  return (
    <DataTable
      label={`Hotspots in ${repo.repo}`}
      rows={repo.files.slice(0, 10)}
      rowKey={(f) => f.file}
      caption={`${repo.repo} @ ${repo.sha.slice(0, 12)}: revisions since ${repo.since} × cyclomatic complexity; ${formatNumber(repo.files_changed)} files changed.`}
      columns={[
        { header: `${repo.repo}: file`, cell: (f) => f.file },
        { header: "Score", cell: (f) => formatNumber(f.score), align: "right" },
        { header: "Revisions", cell: (f) => formatNumber(f.revisions), align: "right" },
        { header: "Complexity", cell: (f) => formatNumber(f.complexity), align: "right" },
        { header: "Churn", cell: (f) => formatNumber(f.churn), align: "right" },
      ]}
    />
  );
}
