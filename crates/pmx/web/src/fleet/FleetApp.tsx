import { Controls } from "../components/layout/Controls";
import { StickyTop } from "../components/layout/StickyTop";
import { ExportMenu } from "../components/layout/ExportMenu";
import { ThemeToggle } from "../components/layout/ThemeToggle";
import { Banner } from "../components/ui/Banner";
import { Section } from "../components/ui/Section";
import layout from "../components/layout/layout.module.css";
import { useFleet } from "./context";
import { useFleetExport } from "./useFleetExport";
import { FleetTable } from "./FleetTable";

function FleetHeader() {
  const { fleet } = useFleet();
  const build = useFleetExport();
  const n = fleet.projects.length;
  return (
    <header className={layout.header}>
      <h1>{fleet.name}</h1>
      <Controls />
      <span className={layout.sub}>
        {n} project{n === 1 ? "" : "s"} · as of {fleet.as_of}
        {fleet.partial_month && " (month in progress)"}
      </span>
      <ExportMenu build={build} />
      <ThemeToggle />
    </header>
  );
}

function FleetNotices() {
  const { fleet, engine } = useFleet();
  const stale = fleet.projects.filter((p) => p.stale);
  return (
    <>
      <Banner>
        The fleet view is for noticing where to look, not for concluding: open a project for the full picture.
      </Banner>
      {stale.length > 0 && (
        <Banner tone="warn">
          Collected before {fleet.as_of}: {stale.map((p) => `${p.name} (as of ${p.as_of})`).join(", ")}. Their
          later days are missing, not zero: run <code>pmx collect</code> there for a like-for-like fleet.
        </Banner>
      )}
      {!engine && (
        <Banner tone="warn">
          This pmx was built without the metrics engine (WASM): only whole weeks and months can be shown.
        </Banner>
      )}
    </>
  );
}

function FleetFooter() {
  const { fleet } = useFleet();
  return (
    <footer className={layout.footer}>
      {fleet.projects.map((p) => (
        <div key={p.key}>
          {p.name} · as of {p.as_of}
          {p.velocity_basis && ` · Velocity scored by ${p.velocity_basis}`}
        </div>
      ))}
      <div>Measured since {fleet.range_start}</div>
      <div>Generated {fleet.generated_at} by pmx · no per-person data on this page</div>
    </footer>
  );
}

/** The fleet page: every project's headlines side by side, each a link into its dashboard. */
export function FleetApp() {
  return (
    <>
      <StickyTop>
        <FleetHeader />
      </StickyTop>
      <FleetNotices />
      <Section title="Headlines">
        <FleetTable />
      </Section>
      <FleetFooter />
    </>
  );
}
