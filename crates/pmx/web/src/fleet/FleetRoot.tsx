import { useEffect, useRef, useState } from "react";
import { App } from "../App";
import type { FleetEngine } from "../data/engine";
import type { FleetData } from "../data/boot";
import { parseHash } from "../lib/urlState";
import { DashboardProvider } from "../state/DashboardProvider";
import { FleetApp } from "./FleetApp";
import { FleetProvider } from "./FleetProvider";

const projectInHash = () => parseHash(window.location.hash).project ?? null;

/** The fleet page, or one of its projects' dashboards when the hash names one (`#p=<key>&…`). */
export function FleetRoot({ data, engine }: { data: FleetData; engine: FleetEngine | null }) {
  const [key, setKey] = useState(projectInHash);
  useEffect(() => {
    const onHash = () => setKey(projectInHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  const shown = useRef<string | null | undefined>(undefined);
  useEffect(() => {
    // Moving between the fleet and a project starts at the top, like a new page.
    if (shown.current !== undefined && shown.current !== key) window.scrollTo(0, 0);
    shown.current = key;
    document.title = `${key && data.projects[key] ? data.projects[key].project.name : data.fleet.name} · pmx`;
  }, [key, data]);

  const project = key ? data.projects[key] : undefined;
  if (key && project) {
    return (
      <DashboardProvider
        key={key}
        project={project}
        leads={null}
        engine={engine?.projects[key] ?? null}
        fleetKey={key}
      >
        <App />
      </DashboardProvider>
    );
  }
  return (
    <FleetProvider fleet={data.fleet} projects={data.projects} engine={engine}>
      <FleetApp />
    </FleetProvider>
  );
}
