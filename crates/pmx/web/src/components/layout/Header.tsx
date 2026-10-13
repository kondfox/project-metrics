import { toHash } from "../../lib/urlState";
import { useDashboard } from "../../state/hooks";
import { useProjectExport } from "../../state/export";
import { Controls } from "./Controls";
import { ExportMenu } from "./ExportMenu";
import { ThemeToggle } from "./ThemeToggle";
import styles from "./layout.module.css";

/** One line: what is shown (the project, its range) stays readable while it sticks to the top. */
export function Header() {
  const { project: p, fleetKey, cadence, range } = useDashboard();
  const build = useProjectExport();
  return (
    <header className={styles.header}>
      {fleetKey && (
        <a className={styles.back} href={toHash(cadence, range)}>
          ← Fleet
        </a>
      )}
      <h1>{p.project.name}</h1>
      <Controls />
      <span className={styles.sub}>
        as of {p.meta.as_of}
        {p.meta.partial_month && " (month in progress)"}
      </span>
      <ExportMenu build={build} />
      <ThemeToggle />
    </header>
  );
}
