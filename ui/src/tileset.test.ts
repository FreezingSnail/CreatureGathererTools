import { describe, expect, it } from "vitest";
import { emptyTileset, sliceCanvas } from "./tileset";

function makeCanvas(w: number, h: number): HTMLCanvasElement {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  return c;
}

describe("sliceCanvas (Go loadTileset parity)", () => {
  it("slices 32x16 into two 16x16 tiles", () => {
    const src = makeCanvas(32, 16);
    const { tiles, perRow } = sliceCanvas(src, 16);
    expect(perRow).toBe(2);
    expect(tiles).toHaveLength(2);
    expect(tiles[0].width).toBe(16);
    expect(tiles[0].height).toBe(16);
  });

  it("handles multi-row sheets row-major", () => {
    const src = makeCanvas(48, 32);
    const { tiles, perRow } = sliceCanvas(src, 16);
    expect(perRow).toBe(3);
    expect(tiles).toHaveLength(6); // 3 cols x 2 rows
  });

  it("drops partial tiles at the edge (Go floor division)", () => {
    const src = makeCanvas(20, 34);
    const { tiles, perRow } = sliceCanvas(src, 16);
    expect(perRow).toBe(1);
    expect(tiles).toHaveLength(2); // 20/16=1 col, 34/16=2 rows
  });

  it("emptyTileset has no tiles", () => {
    const t = emptyTileset();
    expect(t.tiles).toHaveLength(0);
    expect(t.perRow).toBe(0);
  });
});