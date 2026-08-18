import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
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
    const buttons = screen.getAllByTestId("palette-tile");
    expect(buttons).toHaveLength(4);
    // tiles must stay square as they scale (no fixed Button height)
    expect(buttons[0].className).toContain("aspect-square");
    expect(buttons[0].className).toContain("h-auto");
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

  it("overlays an index number on each tile starting at 0", () => {
    const tiles = [makeCanvas(), makeCanvas(), makeCanvas()];
    render(<Palette tiles={tiles} perRow={3} selected={[]} onSelectBlock={() => {}} />);
    const labels = screen.getAllByTestId("palette-tile-index").map((el) => el.textContent);
    expect(labels).toEqual(["0", "1", "2"]);
  });

  it("pinch zoom (ctrl+wheel) scales the grid; plain wheel does not", () => {
    const tiles = [makeCanvas(), makeCanvas(), makeCanvas(), makeCanvas()];
    render(<Palette tiles={tiles} perRow={2} selected={[]} onSelectBlock={() => {}} />);
    const grid = screen.getByTestId("palette");
    fireEvent.wheel(grid, { ctrlKey: true, deltaY: -120 });
    const zoomed = parseFloat(grid.style.zoom);
    expect(zoomed).toBeGreaterThan(1);
    expect(zoomed).toBeLessThan(2.5);
    fireEvent.wheel(grid, { deltaY: 120 });
    expect(parseFloat(grid.style.zoom)).toBe(zoomed);
  });

  it("higher sensitivity zooms further per wheel tick", () => {
    const tiles = [makeCanvas(), makeCanvas(), makeCanvas(), makeCanvas()];
    const { unmount } = render(
      <Palette tiles={tiles} perRow={2} selected={[]} sensitivity={0.001} onSelectBlock={() => {}} />
    );
    let grid = screen.getByTestId("palette");
    fireEvent.wheel(grid, { ctrlKey: true, deltaY: -120 });
    const low = parseFloat(grid.style.zoom);
    unmount();

    render(<Palette tiles={tiles} perRow={2} selected={[]} sensitivity={0.02} onSelectBlock={() => {}} />);
    grid = screen.getByTestId("palette");
    fireEvent.wheel(grid, { ctrlKey: true, deltaY: -120 });
    const high = parseFloat(grid.style.zoom);
    expect(high).toBeGreaterThan(low);
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