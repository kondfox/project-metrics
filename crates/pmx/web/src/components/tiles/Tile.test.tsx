import { render, screen } from "@testing-library/react";
import { Tile } from "./Tile";

describe("<Tile>", () => {
  it("shows a value with its note", () => {
    render(<Tile label="Rework" value="18.4%" note="target 15–25%" />);
    const tile = screen.getByRole("group", { name: "Rework" });
    expect(tile).toHaveTextContent("18.4%");
    expect(tile).toHaveTextContent("target 15–25%");
  });

  it("says 'no data' rather than zero", () => {
    render(<Tile label="Tests with code" state="empty" value="0%" />);
    expect(screen.getByRole("group")).toHaveTextContent("no data");
    expect(screen.getByRole("group")).not.toHaveTextContent("0%");
  });

  it("says 'not measured' for inputs not collected", () => {
    render(<Tile label="Security" state="notMeasured" note="needs osv-scanner" />);
    expect(screen.getByRole("group")).toHaveTextContent("not measured");
  });

  it("flags low n", () => {
    render(<Tile label="Multi-stack devs" value="50%" lowN={{ n: 4, threshold: 10 }} />);
    expect(screen.getByTitle("fewer than 10 events: read with care")).toHaveTextContent("low n · 4");
  });
});
