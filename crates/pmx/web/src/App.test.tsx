import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { DashboardProvider } from "./state/DashboardProvider";
import {
  fixtureLeads,
  fixtureProject,
  fixtureWithPulls,
  fixtureWithSnapshots,
  fixtureWithVelocity,
} from "./test/fixtures";

// ECharts needs a real layout engine; the chart options are tested on their own.
vi.mock("./components/charts/EChart", () => ({ EChart: () => <div data-testid="chart" /> }));

function renderApp(project = fixtureProject(), leads: typeof fixtureLeads | null = null) {
  return render(
    <DashboardProvider project={project} leads={leads} engine={null}>
      <App />
    </DashboardProvider>,
  );
}

beforeEach(() => window.history.replaceState(null, "", "/"));

describe("<App>", () => {
  it("summarizes the last complete month by default", () => {
    renderApp();
    const quality = screen.getByRole("group", { name: "Quality" });
    expect(quality).toHaveTextContent("82");
    expect(within(quality).getByLabelText("band B")).toBeInTheDocument();
    expect(screen.getAllByRole("group", { name: "Rework" })[0]).toHaveTextContent("18.4%");
    expect(screen.getAllByRole("group", { name: "Multi-stack devs" })[0]).toHaveTextContent("low n · 4");
    expect(screen.getAllByRole("group", { name: "Security" })[0]).toHaveTextContent("not measured");
    expect(window.location.hash).toBe("#weekly&2026-09-01..2026-09-30");
  });

  it("shows people only in the lead's private view", () => {
    const { unmount } = renderApp();
    expect(screen.queryByRole("table", { name: "Multi-stack leaders" })).not.toBeInTheDocument();
    unmount();
    renderApp(fixtureProject(), fixtureLeads);
    expect(screen.getByRole("table", { name: "Multi-stack leaders" })).toHaveTextContent("Jane Doe");
  });

  it("switches cadence and keeps it in the URL", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: "Monthly" }));
    expect(screen.getByRole("button", { name: "Monthly" })).toHaveAttribute("aria-pressed", "true");
    expect(window.location.hash).toBe("#monthly&2026-09-01..2026-09-30");
  });

  it("explains ranges it cannot compute without the engine", async () => {
    renderApp();
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Range" }), "3m");
    expect(screen.getByText(/not a whole week or month/)).toBeInTheDocument();
  });

  it("shows snapshot metrics when they were measured", () => {
    renderApp(fixtureWithSnapshots());
    const security = screen.getAllByRole("group", { name: "Security" })[0]!;
    expect(security).toHaveTextContent("~40");
    expect(security).toHaveTextContent("capped at 40");
    expect(within(security).getByLabelText("band D")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Quality" })).toHaveTextContent("4/4 constituents");
    expect(screen.getAllByRole("group", { name: "Duplication" })[0]).toHaveTextContent("4.00%");
    expect(screen.getAllByRole("group", { name: "SAST" })[0]).toHaveTextContent("not measured");
    expect(screen.getByRole("table", { name: "Most vulnerable packages" })).toHaveTextContent("left-pad");
  });

  it("shows peer review when code-host data was collected", () => {
    const { unmount } = renderApp();
    expect(screen.getAllByRole("group", { name: "Peer review" })[0]).toHaveTextContent("not measured");
    unmount();
    renderApp(fixtureWithPulls());
    const [headline, tile] = screen.getAllByRole("group", { name: "Peer review" });
    // September: 20 PRs, none reviewed (0% is a value, not "no data").
    expect(headline).toHaveTextContent("0.0%");
    expect(headline).toHaveTextContent("of 20 merged PRs");
    expect(tile).toHaveTextContent("0.0%");
    expect(screen.getByRole("group", { name: "Cycle time" })).toHaveTextContent("22.3 h");
    expect(screen.getByRole("group", { name: "Review wait" })).toHaveTextContent(
      "no reviewed PRs in this range",
    );
    expect(screen.getByRole("group", { name: "PR size" })).toHaveTextContent("not measured");
    expect(screen.getByText(/from GitLab \(2 repos\)/)).toBeInTheDocument();
  });

  it("shows Velocity and its work packages when a classifier scored them", () => {
    const { unmount } = renderApp();
    expect(screen.getAllByRole("group", { name: "Velocity" })[0]).toHaveTextContent("not measured");
    unmount();
    renderApp(fixtureWithVelocity());
    const [headline, tile] = screen.getAllByRole("group", { name: "Velocity" });
    expect(headline).toHaveTextContent("0.20points per FTE-day");
    expect(headline).toHaveTextContent("points per FTE-day · 2 work packages");
    expect(tile).toHaveTextContent("low n · 2");
    expect(screen.getByRole("group", { name: "Points delivered" })).toHaveTextContent("9");
    const table = screen.getByRole("table", { name: "Work packages" });
    // September only, newest first.
    expect(table).toHaveTextContent("SHOP-42 Order totals");
    expect(table).toHaveTextContent("tidy the checkout form");
    expect(table).not.toHaveTextContent("SHOP-40");
    expect(screen.getByText(/six-axis@2 rubric by claude-code:claude-sonnet-5-5/)).toBeInTheDocument();
    expect(screen.queryByRole("table", { name: "Points by person" })).not.toBeInTheDocument();
  });

  it("shows points by person only in the lead's private view", () => {
    renderApp(fixtureWithVelocity(), fixtureLeads);
    const table = screen.getByRole("table", { name: "Points by person" });
    expect(table).toHaveTextContent("Jane Doe");
    expect(table).toHaveTextContent("4.5");
  });

  it("hides AI series when attribution is off", () => {
    renderApp(fixtureProject({ ai_attribution: false }));
    expect(screen.getByRole("group", { name: "AI-assisted" })).toHaveTextContent("not tracked");
    expect(screen.queryByRole("table", { name: "AI-assisted vs human" })).not.toBeInTheDocument();
  });
});
