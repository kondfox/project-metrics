import type { ReactNode } from "react";
import styles from "./ui.module.css";

export function Banner({
  tone = "info",
  children,
}: {
  tone?: "info" | "warn" | "error";
  children: ReactNode;
}) {
  return (
    <div
      className={`${styles.banner} ${tone === "info" ? "" : styles[tone]}`}
      role={tone === "error" ? "alert" : "note"}
    >
      {children}
    </div>
  );
}
