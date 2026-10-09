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
});
