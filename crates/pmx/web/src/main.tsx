import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { loadPageData } from "./data/boot";
import { loadEngine, loadFleetEngine } from "./data/engine";
import { FleetRoot } from "./fleet/FleetRoot";
import { DashboardProvider } from "./state/DashboardProvider";
import "./styles/theme.css";

/** The engine is optional: without it the page shows whole weeks and months only. */
async function orNull<T>(p: Promise<T | null>): Promise<T | null> {
  try {
    return await p;
  } catch (e) {
    console.error("metrics engine failed to load", e);
    return null;
  }
}

async function main() {
  const root = createRoot(document.getElementById("root")!);
  try {
    const data = await loadPageData();
    if (data.kind === "fleet") {
      const engine = await orNull(loadFleetEngine(data.wasm, data.fleet, data.projects));
      root.render(
        <StrictMode>
          <FleetRoot data={data} engine={engine} />
        </StrictMode>,
      );
    } else {
      document.title = `${data.project.project.name} · pmx`;
      const engine = await orNull(loadEngine(data.wasm, data.project));
      root.render(
        <StrictMode>
          <DashboardProvider project={data.project} leads={data.leads} engine={engine}>
            <App />
          </DashboardProvider>
        </StrictMode>,
      );
    }
  } catch (e) {
    root.render(<pre role="alert">{String(e)}</pre>);
  } finally {
    // For automated checks (headless screenshots, tests).
    document.body.dataset.ready = "1";
  }
}

void main();
