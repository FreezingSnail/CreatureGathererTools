import { describe, expect, it } from "vitest";
import {
  screenToTile,
  tileToPixel,
  clampOffset,
  clampZoom,
  brushCells,
  blockCells,
  chunkId,
  selectRect,
  parseHexColor,
  zoneColor,
  TILE_SIZE,
  MARGIN_X,
  MARGIN_Y,
  CHUNK_COLS,
  CHUNK_W,
  CHUNK_H,
} from "./geometry";

describe("screenToTile / tileToPixel", () => {
  it("round-trips at zoom 2", () => {
    const zoom = 2;
    for (const [col, row] of [
      [0, 0],
      [10, 5],
      [200, 100],
    ] as const) {
      const { px, py } = tileToPixel(col, row, 10, 20, zoom);
      // top-left corner maps back exactly
      expect(screenToTile(px, py, 10, 20, zoom)).toEqual({ col, row });
    }
  });

  it("maps screen coords with margins", () => {
    // (MARGIN_X, MARGIN_Y) with zoom 1 is tile (offsetX, offsetY)
    expect(screenToTile(MARGIN_X, MARGIN_Y, 3, 4, 1)).toEqual({ col: 3, row: 4 });
    // half a tile into the margin area still lands on the offset tile
    const pos = tileToPixel(5, 6, 3, 4, 2);
    expect(screenToTile(pos.px + TILE_SIZE * 2 * 0.5, pos.py, 3, 4, 2)).toEqual({
      col: 5,
      row: 6,
    });
  });
});

describe("clamps", () => {
  it("offset never negative", () => {
    expect(clampOffset(-1, 5)).toEqual({ col: 0, row: 5 });
    expect(clampOffset(3, -2)).toEqual({ col: 3, row: 0 });
  });
  it("zoom 0.5..8", () => {
    expect(clampZoom(0.1)).toBe(0.5);
    expect(clampZoom(99)).toBe(8);
    expect(clampZoom(1.5)).toBe(1.5);
  });
});

describe("brushes and blocks", () => {
  it("brush size 1 hits one cell", () => {
    expect(brushCells(5, 5, 1)).toEqual([{ col: 5, row: 5 }]);
  });
  it("brush size 2 hits the same 3x3 as Go (r = floor(2/2) = 1)", () => {
    const cells = brushCells(5, 5, 2);
    expect(cells).toHaveLength(9);
    expect(cells).toContainEqual({ col: 5, row: 5 });
    expect(cells).toContainEqual({ col: 4, row: 4 });
    expect(cells).toContainEqual({ col: 6, row: 6 });
  });
  it("brush size 3 hits 3x3 centered", () => {
    expect(brushCells(5, 5, 3)).toHaveLength(9);
  });
  it("block cells are row-major", () => {
    expect(blockCells(1, 1, 2, 3)).toEqual([
      { col: 1, row: 1 },
      { col: 2, row: 1 },
      { col: 1, row: 2 },
      { col: 2, row: 2 },
      { col: 1, row: 3 },
      { col: 2, row: 3 },
    ]);
  });
});

describe("chunks", () => {
  it("chunkId matches 8x4 layout", () => {
    expect(chunkId(0, 0)).toBe(0);
    expect(chunkId(7, 0)).toBe(0);
    expect(chunkId(8, 0)).toBe(1);
    expect(chunkId(0, 3)).toBe(0);
    expect(chunkId(0, 4)).toBe(CHUNK_COLS);
    expect(chunkId(CHUNK_W * 3, CHUNK_H * 7)).toBe(3 + 7 * CHUNK_COLS);
    // 2048 chunks total on a 256-map
    expect(chunkId(255, 255)).toBe(32 * 64 - 1);
  });
});

describe("selectRect (palette rectangle select)", () => {
  const total = 16;
  it("single index", () => {
    expect(selectRect(3, 3, 4, total)).toEqual({ block: [3], w: 1, h: 1 });
  });
  it("row-major rectangle across rows", () => {
    const { block, w, h } = selectRect(1, 10, 4, total);
    expect(w).toBe(2);
    expect(h).toBe(3);
    expect(block).toEqual([1, 2, 5, 6, 9, 10]);
  });
  it("handles reversed anchor order", () => {
    const { block, w, h } = selectRect(10, 1, 4, total);
    expect({ block, w, h }).toEqual(selectRect(1, 10, 4, total));
  });
  it("clamps out-of-range indices to -1", () => {
    const { block } = selectRect(14, 15, 4, total);
    expect(block).toEqual([14, 15]);
    const out = selectRect(15, 20, 4, total);
    // rows 3..5 (row 5 fully out of range), cols 0..3
    expect(out.block).toEqual([
      12, 13, 14, 15, -1, -1, -1, -1, -1, -1, -1, -1,
    ]);
  });
});

describe("colors", () => {
  it("parses hex RGBA and RGB", () => {
    expect(parseHexColor("#ff000080")).toEqual([255, 0, 0, 128]);
    expect(parseHexColor("#00ff00")).toEqual([0, 255, 0, 128]);
  });
  it("zoneColor cycles and hides negative", () => {
    expect(zoneColor(0)).toEqual([100, 220, 100, 100]);
    expect(zoneColor(-1)).toEqual([0, 0, 0, 0]);
    expect(zoneColor(6)).toEqual(zoneColor(0)); // cycles
  });
});