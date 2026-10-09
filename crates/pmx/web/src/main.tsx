import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { loadDashboardData } from "./data/boot";
import { loadEngine } from "./data/engine";
import { DashboardProvider } from "./state/DashboardProvider";
import "./styles/theme.css";

async function main() {
  const root = createRoot(document.getElementById("root")!);
  try {
    const data = await loadDashboardData();
    document.title = `${data.project.project.name} · pmx`;
    let engine = null;
    try {
      engine = await loadEngine(data.wasm, data.project);
    } catch (e) {
      console.error("metrics engine failed to load", e);
    }
    root.render(
      <StrictMode>
        <DashboardProvider project={data.project} leads={data.leads} engine={engine}>
          <App />
        </DashboardProvider>
      </StrictMode>,
    );
  } catch (e) {
    root.render(<pre role="alert">{String(e)}</pre>);
  } finally {
    // For automated checks (headless screenshots, tests).
    document.body.dataset.ready = "1";
  }
}

void main();
