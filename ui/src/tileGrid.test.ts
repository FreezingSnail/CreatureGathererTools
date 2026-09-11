import { describe, expect, it } from "vitest";
import { createGrid, decodeGrid, encodeGrid, fillGrid, resizeGrid, resizeGridFromHandle, stampGrid } from "./tileGrid";

describe("tileGrid", () => {
  it("creates a grid of the requested size", () => {
    expect(createGrid(3, 2, 7)).toEqual({ w: 3, h: 2, tiles: [7, 7, 7, 7, 7, 7] });
  });

  it("clamps degenerate sizes to at least one cell", () => {
    expect(createGrid(0, -3)).toEqual({ w: 1, h: 1, tiles: [0] });
  });

  it("keeps the top-left overlap when resizing", () => {
    const grid = { w: 2, h: 2, tiles: [1, 2, 3, 4] };
    expect(resizeGrid(grid, 3, 1)).toEqual({ w: 3, h: 1, tiles: [1, 2, 0] });
    expect(resizeGrid(grid, 3, 3)).toEqual({ w: 3, h: 3, tiles: [1, 2, 0, 3, 4, 0, 0, 0, 0] });
  });

  it("stamps a brush block and clips at the edges", () => {
    const grid = createGrid(3, 2);
    expect(stampGrid(grid, 1, 0, [8, 9, 10, 11], 2, 2).tiles).toEqual([0, 8, 9, 0, 10, 11]);
    expect(stampGrid(grid, 2, 1, [8, 9, 10, 11], 2, 2).tiles).toEqual([0, 0, 0, 0, 0, 8]);
  });

  it("floods a grid with one tile", () => {
    expect(fillGrid(createGrid(2, 2), 5).tiles).toEqual([5, 5, 5, 5]);
  });

  it("round-trips through the run-length codec", () => {
    const grid = { w: 2, h: 2, tiles: [4, 4, 4, 9] };
    const encoded = encodeGrid(grid);
    expect(encoded).toBe("4x3,9");
    expect(decodeGrid(encoded, 2, 2)).toEqual(grid);
  });

  it("falls back to an empty grid when a payload does not match its extent", () => {
    expect(decodeGrid("4,5", 2, 2)).toEqual({ w: 2, h: 2, tiles: [0, 0, 0, 0] });
  });
});

  it("supports arbitrary dimensions without an authoring cap", () => {
    expect(createGrid(40, 37).tiles).toHaveLength(1480);
  });

  it("grows from west and north while translating existing content", () => {
    const source = { w: 2, h: 2, tiles: [1, 2, 3, 4] };
    expect(resizeGridFromHandle(source, "nw", -1, -1).grid).toEqual({ w: 3, h: 3, tiles: [0, 0, 0, 0, 1, 2, 0, 3, 4] });
  });

  it("reports only non-empty discarded cells", () => {
    const source = { w: 3, h: 2, tiles: [7, 0, 0, 0, 0, 0] };
    const empty = resizeGridFromHandle(source, "e", -2, 0);
    expect(empty.discardedNonEmpty).toBe(0);
    const nonEmpty = resizeGridFromHandle({ ...source, tiles: [7, 8, 0, 0, 0, 0] }, "e", -2, 0);
    expect(nonEmpty.discardedNonEmpty).toBe(1);
  });

  it("clamps handle shrinking to one cell", () => {
    const result = resizeGridFromHandle({ w: 3, h: 2, tiles: [1, 2, 3, 4, 5, 6] }, "se", -99, -99);
    expect(result.grid.w).toBe(1);
    expect(result.grid.h).toBe(1);
    expect(result.discardedNonEmpty).toBe(5);
  });
