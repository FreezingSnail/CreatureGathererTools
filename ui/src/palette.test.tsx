import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { Palette } from "./palette";

function makeCanvas(): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = 16;
  c.height = 16;
  return c;
}

describe("Palette", () => {
  it("renders one button per tile in grid columns", () => {
    const tiles = [makeCanvas(), makeCanvas(), makeCanvas(), makeCanvas()];
    render(<Palette tiles={tiles} perRow={2} selected={[0]} onSelectBlock={() => {}} />);
    expect(screen.getAllByTestId("palette-tile")).toHaveLength(4);
  });

  it("shows empty label when no tileset", () => {
    render(
      <Palette
        tiles={[]}
        perRow={4}
        selected={[]}
        emptyLabel="No map tileset loaded"
        onSelectBlock={() => {}}
      />
    );
    expect(screen.getByText("No map tileset loaded")).toBeInTheDocument();
  });

  it("emits a single-select block on plain click", () => {
    const tiles = [makeCanvas(), makeCanvas()];
    let captured: { block: number[]; w: number; h: number } | null = null;
    render(
      <Palette
        tiles={tiles}
        perRow={2}
        selected={[]}
        onSelectBlock={(block, w, h) => {
          captured = { block, w, h };
        }}
      />
    );
    screen.getAllByTestId("palette-tile")[1].click();
    expect(captured).toEqual({ block: [1], w: 1, h: 1 });
  });
});