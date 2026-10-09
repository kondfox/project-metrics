import { useDashboard } from "../../state/hooks";
import styles from "./layout.module.css";

export function Footer() {
  const { project: p, leads } = useDashboard();
  return (
    <footer className={styles.footer}>
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
