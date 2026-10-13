import { useTheme } from "../../lib/theme";
import styles from "./layout.module.css";

/** Dark / light mode, at the end of the header. */
export function ThemeToggle() {
  const [theme, toggle] = useTheme();
  const next = theme === "dark" ? "light" : "dark";
  return (
    <button
      type="button"
      className={styles.themeToggle}
      data-noprint
      onClick={toggle}
      aria-label={`Switch to ${next} mode`}
      title={`Switch to ${next} mode`}
    >
      {theme === "dark" ? "☀" : "☾"}
    </button>
  );
}
