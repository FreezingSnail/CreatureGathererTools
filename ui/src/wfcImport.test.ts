import { describe, expect, it } from "vitest";
import type { Block, WfcPiece } from "./types";
import { decode, encode } from "./rle";
import {
  alignmentMessage,
  blockToPiece,
  blocksToPieces,
  fillPiecesForTiles,
  fillTileOf,
  isSlotAligned,
  missingFillTiles,
  sheetSelectionToPiece,
} from "./wfcImport";

const block = (overrides: Partial<Block> = {}): Block => ({
  id: 1,
  name: "hut",
  w: 2,
  h: 2,
  tiles: "4,5,6,7",
  tags: [],
  ...overrides,
});

const piece = (overrides: Partial<WfcPiece> = {}): WfcPiece => ({
  id: 1,
  name: "piece",
  tw: 2,
  th: 2,
  tiles: encode([4, 4, 4, 4]),
  weight: 1,
  limit: null,
  edge_overrides: [],
  tags: [],
  ...overrides,
});

describe("wfcImport alignment", () => {
  it("accepts blocks whose extents are slot multiples", () => {
    expect(isSlotAligned(block({ w: 4, h: 2 }), 2, 2)).toBe(true);
    expect(isSlotAligned(block({ w: 3, h: 2 }), 2, 2)).toBe(false);
  });

  it("explains misalignment with both sizes", () => {
    expect(alignmentMessage(block({ w: 3, h: 2 }), 2, 2)).toBe("hut is 3×2; WFC needs multiples of 2×2.");
    expect(alignmentMessage(block(), 2, 2)).toBeNull();
  });
});

describe("wfcImport pieces", () => {
  it("imports an aligned block as a piece tagged block", () => {
    const result = blockToPiece([piece()], block(), 2, 2);
    expect("piece" in result).toBe(true);
    if (!("piece" in result)) return;
    expect(result.piece).toMatchObject({ id: 2, name: "hut", tw: 2, th: 2, weight: 1, tags: ["block"] });
    expect(decode(result.piece.tiles, 4)).toEqual([4, 5, 6, 7]);
  });

  it("refuses a misaligned block", () => {
    const result = blockToPiece([], block({ w: 3 }), 2, 2);
    expect(result).toEqual({ error: "hut is 3×2; WFC needs multiples of 2×2." });
  });

  it("imports the aligned subset and reports each skipped block", () => {
    const { created, errors } = blocksToPieces([], [
      block({ id: 1, name: "good" }),
      block({ id: 2, name: "odd", w: 3 }),
      block({ id: 3, name: "wide", w: 4 }),
    ], 2, 2);
    expect(created.map((item) => [item.id, item.name])).toEqual([[1, "good"], [2, "wide"]]);
    expect(errors).toEqual(["odd is 3×2; WFC needs multiples of 2×2."]);
  });
});

describe("wfcImport fill pieces", () => {
  it("reports the fill tile only for uniform pieces", () => {
    expect(fillTileOf(piece())).toBe(4);
    expect(fillTileOf(piece({ tiles: encode([4, 5, 4, 4]) }))).toBeNull();
  });

  it("lists uncovered sheet tiles in ascending order", () => {
    expect(missingFillTiles([piece()], [2, 4, 1, 1])).toEqual([1, 2]);
  });

  it("creates slot-sized fill pieces and skips covered tiles", () => {
    const created = fillPiecesForTiles([piece()], [4, 0, 9], 2, 2);
    expect(created.map((item) => item.name)).toEqual(["Fill 0", "Fill 9"]);
    expect(created.map((item) => item.id)).toEqual([2, 3]);
    expect(created[0]).toMatchObject({ tw: 2, th: 2, tags: ["fill"] });
    expect(decode(created[1]!.tiles, 4)).toEqual([9, 9, 9, 9]);
  });
});


describe("wfcImport sprite-sheet capture", () => {
  it("encodes an aligned exact rectangle and allocates the next id", () => {
    const result = sheetSelectionToPiece([piece({ id: 8 })], [1, 2, 3, 4], 2, 2, 2, 2, "Cave");
    expect("piece" in result).toBe(true);
    if ("error" in result) return;
    expect(result.piece).toMatchObject({ id: 9, name: "Cave", tw: 2, th: 2, tags: ["sheet"] });
    expect(decode(result.piece.tiles, 4)).toEqual([1, 2, 3, 4]);
  });

  it("rejects incomplete and non-aligned rectangles without padding", () => {
    expect(sheetSelectionToPiece([], [1, 2, -1], 2, 2, 2, 2, "bad")).toEqual({ error: "Sheet selection is incomplete; expected 4 tiles but received 3." });
    expect(sheetSelectionToPiece([], [1, 2, 3], 3, 1, 2, 1, "bad")).toEqual({ error: "Sheet selection 3×1 must be exact multiples of the 2×1 WFC slot." });
  });
});
