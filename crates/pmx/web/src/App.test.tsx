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
    expect(within(quality).getByRole("link", { name: "How Quality is measured" })).toHaveAttribute(
      "href",
      expect.stringMatching(/wiki\/metrics-spec\.md#3-quality-/),
    );
    expect(screen.getAllByRole("group", { name: "Rework" })[0]).toHaveTextContent("18.4%");
    // People counts are never flagged low-n: a team rarely has 10 developers.
    expect(screen.getAllByRole("group", { name: "Multi-stack devs" })[0]).not.toHaveTextContent("low n");
    expect(screen.getAllByRole("group", { name: "Security" })[0]).toHaveTextContent("not measured");
    expect(window.location.hash).toBe("#weekly&2026-09-01..2026-09-30");
  });

  it("opens a headline's section from its tile", async () => {
    const scrolled: Element[] = [];
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    renderApp();
    const headline = screen.getByRole("region", { name: "Headline" });
    await userEvent.click(within(headline).getByRole("button", { name: "Peer review" }));
    expect(scrolled).toEqual([screen.getByRole("region", { name: "Peer review" })]);
    expect(window.location.hash).toBe("#weekly&2026-09-01..2026-09-30");
  });

  it("exports the range's metrics as CSV and every table as XLSX", async () => {
    const files: Array<{ name: string; blob: Blob }> = [];
    vi.spyOn(URL, "createObjectURL").mockImplementation((b) => {
      files.push({ name: "", blob: b as Blob });
      return "blob:export";
    });
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
      files.at(-1)!.name = this.download;
    });
    window.history.replaceState(null, "", "#monthly&2026-09-01..2026-09-30");
    renderApp(fixtureWithVelocity());
    await userEvent.click(screen.getByRole("button", { name: "Export ▾" }));
    await userEvent.click(screen.getByRole("menuitem", { name: /^CSV/ }));
    expect(files[0]!.name).toBe("acme-shop_2026-09-01_2026-09-30.csv");
    const csv = await files[0]!.blob.text();
    // (Blob.text() drops the BOM; toCsv writes it, tested in lib/export.)
    expect(csv.split("\r\n")[0]).toBe("Project,From,To,Section,Metric,Value,Unit,Events (n),Metric id");
    expect(csv).toContain("Acme Shop,2026-09-01,2026-09-30,Quality,Quality,");
    expect(csv).toContain(",FTE (mean over business days),2,FTE,,fte");
    await userEvent.click(screen.getByRole("button", { name: "Export ▾" }));
    await userEvent.click(screen.getByRole("menuitem", { name: /^Excel/ }));
    expect(files[1]!.name).toBe("acme-shop_2026-09-01_2026-09-30.xlsx");
    expect(files[1]!.blob.type).toBe("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet");
    vi.restoreAllMocks();
  });

  it("switches between light and dark mode and remembers it", async () => {
    renderApp();
    await userEvent.click(screen.getByRole("button", { name: "Switch to dark mode" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(localStorage.getItem("pmx.theme")).toBe("dark");
    await userEvent.click(screen.getByRole("button", { name: "Switch to light mode" }));
    expect(document.documentElement.dataset.theme).toBe("light");
    localStorage.removeItem("pmx.theme");
    delete document.documentElement.dataset.theme;
  });

  it("shows people only in the lead's private view", async () => {
    const { unmount } = renderApp();
    expect(screen.queryByRole("table", { name: "Multi-stack leaders" })).not.toBeInTheDocument();
    unmount();
    renderApp(fixtureProject(), fixtureLeads);
    const leaders = screen.getByRole("table", { name: "Multi-stack leaders" });
    expect(leaders).toHaveTextContent("Jane Doe");
    // Each stack a badge in its role's color.
    expect(within(leaders).getByText("Backend")).toHaveStyle({ "--role": "var(--series-2)" });
    await userEvent.hover(within(leaders).getByRole("button", { name: "what this means" }));
    expect(screen.getByRole("tooltip")).toHaveTextContent("How evenly the person's added lines spread");
    await userEvent.unhover(within(leaders).getByRole("button", { name: "what this means" }));
    const breadth = screen.getByRole("group", { name: "Breadth index" });
    await userEvent.hover(within(breadth).getByRole("button", { name: "what this means" }));
    expect(screen.getByRole("tooltip")).toHaveTextContent("The mean of everyone's Breadth");
    // In alphabetical order (the fixture lists frontend first).
    expect(within(leaders).getByText("Backend").nextElementSibling).toHaveTextContent("Frontend");
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
    expect(screen.getByRole("group", { name: "FTE" })).toHaveTextContent(
      "2.044.0 FTE-days over 22 business days",
    );
    expect(screen.getByRole("figure", { name: "FTE" })).toBeInTheDocument();
    expect(screen.getByRole("figure", { name: "Work packages by size" })).toBeInTheDocument();
    const table = screen.getByRole("table", { name: "Work packages" });
    // September only, newest first.
    expect(table).toHaveTextContent("SHOP-42 Order totals");
    expect(table).toHaveTextContent("tidy the checkout form");
    expect(table).not.toHaveTextContent("SHOP-40");
    expect(screen.getByText(/six-axis@3 rubric by claude-code:claude-sonnet-5-5/)).toBeInTheDocument();
    expect(screen.getByText(/Grouped: 2 by ticket, 0 by PR \/ MR, 1 by author-week/)).toHaveTextContent(
      "No PR data for shop-web",
    );
    expect(screen.queryByRole("table", { name: "Points by person" })).not.toBeInTheDocument();
  });

  it("shows the speed-ups and what to configure", () => {
    window.history.replaceState(null, "", "#monthly&2026-09-01..2026-09-30");
    renderApp(fixtureWithVelocity());
    const phase = screen.getByRole("figure", { name: "Against the 12 weeks before" });
    expect(phase).toHaveTextContent("No phases set: add [[phase]] to pmx.toml");
    expect(within(phase).getByLabelText("Velocity ×1.8")).toHaveTextContent("90%: ×1.2–×2.4");
    expect(within(phase).getByLabelText("Volume ×1.1")).toHaveTextContent("no clear change");
    expect(phase).toHaveTextContent("Rework +6.2 pts");
    expect(phase).toHaveTextContent("Between the windows: “Summer slowdown”. Read with care.");
    const marker = screen.getByRole("figure", { name: "Since “CLAUDE.md added”" });
    expect(marker).toHaveTextContent("Too early: 2 complete weeks since 2026-09-08");
    expect(marker).toHaveTextContent('confirm it with [[marker]] kind = "ai"');
  });

  it("asks for an AI marker when there is none", () => {
    window.history.replaceState(null, "", "#monthly&2026-09-01..2026-09-30");
    const p = fixtureWithVelocity();
    p.speed_ups[0]!.markers = [];
    renderApp(p);
    expect(screen.getByRole("figure", { name: "No AI marker" })).toHaveTextContent(
      'add [[marker]] with kind = "ai" to pmx.toml',
    );
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
