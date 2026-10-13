import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { LeadsFile } from "../data/types";
import { fixtureFleet, fixtureLeads } from "../test/fixtures";
import { FleetRoot } from "./FleetRoot";

// ECharts needs a real layout engine; the chart options are tested on their own.
vi.mock("../components/charts/EChart", () => ({ EChart: () => <div data-testid="chart" /> }));

function renderFleet(leads: Record<string, LeadsFile> | null = null) {
  const { fleet, projects } = fixtureFleet();
  return render(<FleetRoot data={{ kind: "fleet", fleet, projects, leads, wasm: null }} engine={null} />);
}

const row = (name: string) => screen.getByRole("row", { name: new RegExp(`^${name}`) });

beforeEach(() => window.history.replaceState(null, "", "/"));

describe("<FleetRoot>", () => {
  it("shows a row per project and the pooled fleet row", () => {
    renderFleet();
    expect(screen.getByRole("heading", { name: "Acme Engineering" })).toBeInTheDocument();
    const shop = row("Acme Shop");
    expect(shop).toHaveTextContent("82");
    expect(within(shop).getByLabelText("band B")).toBeInTheDocument();
    expect(shop).toHaveTextContent("~40");
    const crm = row("Acme CRM");
    expect(crm).toHaveTextContent("49");
    expect(crm).toHaveTextContent("as of 2026-09-29");
    expect(within(crm).getAllByText("not measured")).toHaveLength(4); // velocity, security, AI, PRs
    const fleet = row("Fleet");
    expect(fleet).toHaveTextContent("63");
    expect(fleet).toHaveTextContent("1/2"); // AI, peer review and security from one project
    expect(screen.getByText(/Projects by band/)).toHaveTextContent("Quality: 1×B 1×D");
    expect(screen.getByText(/Collected before 2026-09-30/)).toHaveTextContent("Acme CRM");
  });

  it("puts the fleet row first, then the projects by name", () => {
    renderFleet();
    const names = screen
      .getAllByRole("row")
      .slice(1)
      .map((r) => r.querySelector("th")?.textContent);
    expect(names.map((n) => n?.replace(/as of.*/, ""))).toEqual(["Fleet", "Acme CRM", "Acme Shop"]);
  });

  it("exports the table as shown, fleet row first, with each value's change", async () => {
    let blob: Blob | null = null;
    vi.spyOn(URL, "createObjectURL").mockImplementation((b) => ((blob = b as Blob), "blob:export"));
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    window.history.replaceState(null, "", "#monthly&2026-09-01..2026-09-30");
    renderFleet();
    await userEvent.click(screen.getByRole("button", { name: "Export ▾" }));
    await userEvent.click(screen.getByRole("menuitem", { name: /^CSV/ }));
    const lines = (await blob!.text()).split("\r\n");
    expect(lines[0]).toMatch(/^Row,From,To,As of,Velocity,Velocity change,Quality,Quality change,/);
    expect(lines.slice(1, 4).map((l) => l.split(",")[0])).toEqual(["Fleet", "Acme CRM", "Acme Shop"]);
    // Fleet quality 60 → 63.2.
    expect(lines[1]!.split(",").slice(6, 8).map(Number)).toEqual([63.2, expect.closeTo(3.2, 6)]);
    vi.restoreAllMocks();
  });

  it("sorts the projects by any column, keeping the fleet row first", async () => {
    renderFleet();
    const order = () =>
      screen
        .getAllByRole("row")
        .slice(1)
        .map((r) => r.querySelector("th")?.textContent?.replace(/as of.*/, ""));
    const header = (name: string) => screen.getByRole("button", { name: new RegExp(`^${name}`) });
    await userEvent.click(header("Quality"));
    expect(order()).toEqual(["Fleet", "Acme Shop", "Acme CRM"]); // 82, 49: highest first
    expect(header("Quality").closest("th")).toHaveAttribute("aria-sort", "descending");
    await userEvent.click(header("Quality"));
    expect(order()).toEqual(["Fleet", "Acme CRM", "Acme Shop"]);
    // CRM doesn't measure security: last either way.
    await userEvent.click(header("Security"));
    expect(order()).toEqual(["Fleet", "Acme Shop", "Acme CRM"]);
    await userEvent.click(header("Security"));
    expect(order()).toEqual(["Fleet", "Acme Shop", "Acme CRM"]);
    await userEvent.click(header("Project"));
    expect(order()).toEqual(["Fleet", "Acme CRM", "Acme Shop"]);
    await userEvent.click(header("Project"));
    expect(order()).toEqual(["Fleet", "Acme Shop", "Acme CRM"]);
  });

  it("draws a trend line under each value", () => {
    window.history.replaceState(null, "", "#monthly&2026-09-01..2026-09-30");
    renderFleet();
    expect(within(row("Acme CRM")).getByRole("img", { name: "Quality trend" })).toBeInTheDocument();
    expect(within(row("Fleet")).getByRole("img", { name: "Security trend" })).toBeInTheDocument();
  });

  it("shows how much each value moved since the period before", () => {
    window.history.replaceState(null, "", "#monthly&2026-09-01..2026-09-30");
    renderFleet();
    const fleet = row("Fleet");
    expect(within(fleet).getByTitle("up from 60 in the period before")).toHaveTextContent("▲3");
    expect(within(fleet).getByTitle("down from 52 in the period before")).toHaveTextContent("▼12");
    expect(within(row("Acme CRM")).getByTitle("up from 10.0% in the period before")).toHaveTextContent("▲10");
  });

  it("opens a project's dashboard and comes back", async () => {
    renderFleet();
    await userEvent.click(screen.getByRole("link", { name: "Acme CRM" }));
    expect(window.location.hash).toBe("#p=crm&weekly&2026-09-01..2026-09-30");
    expect(await screen.findByRole("heading", { name: "Acme CRM" })).toBeInTheDocument();
    expect(screen.getAllByRole("group", { name: "AI-assisted" })[0]).toHaveTextContent("not measured");
    await userEvent.click(screen.getByRole("link", { name: "← Fleet" }));
    expect(await screen.findByRole("table", { name: "Fleet" })).toBeInTheDocument();
    expect(window.location.hash).toBe("#weekly&2026-09-01..2026-09-30");
  });

  it("keeps the preset when a link carries its range", () => {
    window.history.replaceState(null, "", "#p=shop&monthly&2026-09-01..2026-09-30");
    renderFleet();
    expect(screen.getByRole("heading", { name: "Acme Shop" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Range" })).toHaveValue("month");
  });

  it("shows a project's people only in the lead's own copy", () => {
    window.history.replaceState(null, "", "#p=shop&monthly&2026-09-01..2026-09-30");
    const { unmount } = renderFleet();
    expect(screen.queryByRole("table", { name: "Multi-stack leaders" })).not.toBeInTheDocument();
    unmount();
    renderFleet({ shop: fixtureLeads });
    expect(screen.getByRole("table", { name: "Multi-stack leaders" })).toHaveTextContent("Jane Doe");
  });

  it("ignores an unknown project", () => {
    window.history.replaceState(null, "", "#p=nope");
    renderFleet();
    expect(screen.getByRole("table", { name: "Fleet" })).toBeInTheDocument();
  });
});
