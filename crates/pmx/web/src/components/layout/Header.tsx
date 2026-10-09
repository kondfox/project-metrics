import { useDashboard } from "../../state/hooks";
import styles from "./layout.module.css";

export function Header() {
  const { project: p } = useDashboard();
  const repos = p.project.repos.length;
  return (
    <header className={styles.header}>
      <h1>{p.project.name}</h1>
      <span className={styles.sub}>
        as of {p.meta.as_of}
        {p.meta.partial_month && " (month in progress)"} · {repos} repo{repos === 1 ? "" : "s"} · since{" "}
        {p.project.range_start}
      </span>
    </header>
  );
}
