import { render, screen, within } from "@testing-library/react";
import { DataTable } from "./DataTable";

describe("<DataTable>", () => {
  it("renders typed rows and columns", () => {
    const rows = [
      { name: "Ann", lines: 1200 },
      { name: "Bo", lines: 40 },
    ];
    render(
      <DataTable
        label="people"
        rows={rows}
        rowKey={(r) => r.name}
        columns={[
          { header: "Person", cell: (r) => r.name },
          { header: "Lines", cell: (r) => r.lines, align: "right" },
        ]}
        caption="lead only"
      />,
    );
    const table = screen.getByRole("table", { name: "people" });
    const body = within(table).getAllByRole("row").slice(1);
    expect(body.map((r) => r.textContent)).toEqual(["Ann1200", "Bo40"]);
    expect(table).toHaveTextContent("lead only");
  });

  it("scrolls in a box of `visibleRows` rows, with every row rendered and the caption below", () => {
    const rows = Array.from({ length: 30 }, (_, i) => ({ n: i }));
    render(
      <DataTable
        label="many"
        rows={rows}
        rowKey={(r) => String(r.n)}
        columns={[{ header: "N", cell: (r) => r.n }]}
        caption="30 rows"
        visibleRows={10}
      />,
    );
    const table = screen.getByRole("table", { name: "many" });
    expect(within(table).getAllByRole("row")).toHaveLength(31);
    expect(table).not.toHaveTextContent("30 rows");
    expect(screen.getByText("30 rows")).toBeInTheDocument();
  });
});
