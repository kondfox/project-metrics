import { useDashboard } from "../../state/hooks";
import styles from "./layout.module.css";

export function Footer() {
  const { project: p, leads } = useDashboard();
  const repos = p.project.repos.length;
  return (
    <footer className={styles.footer}>
      <div>
        Charts: * the period is still running · hollow = low n (few events: read with care) · ~ point in time
      </div>
      <div>
        {repos} repo{repos === 1 ? "" : "s"} · measured since {p.project.range_start}
      </div>
      {p.project.repos.map((r) => (
        <div key={r.name}>
          {r.name} · {r.role} · <code>{r.sha.slice(0, 12)}</code> · {r.tip_date}
        </div>
      ))}
      <div>
        Generated {p.meta.generated_at} by pmx {p.meta.tool_versions.pmx ?? ""} ·{" "}
        {p.meta.tool_versions.git ?? ""}
        {leads && " · private lead view"}
      </div>
    </footer>
  );
}
