import { describe, expect, it } from "vitest";
import type { Block, GameMap } from "./types";
import {
  blockClipboard,
  blockFromGrid,
  blockGrid,
  blockLabel,
  duplicateBlock,
  gridFromMap,
  nextBlockId,
  removeBlock,
  upsertBlock,
  withGrid,
} from "./blocks";

const block = (overrides: Partial<Block> = {}): Block => ({
  id: 1,
  name: "hut",
  w: 2,
  h: 2,
  tiles: "4,5,6,7",
  tags: [],
  ...overrides,
});

const map = (): GameMap => ({
  width: 3,
  height: 3,
  tiles: [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
  npcs: [],
  scripts: [],
  collision: [],
  collision_types: [],
  encounters: [],
  encounter_zones: [],
});

describe("blocks", () => {
  it("allocates ids above the current maximum", () => {
    expect(nextBlockId([])).toBe(1);
    expect(nextBlockId([block({ id: 4 }), block({ id: 2 })])).toBe(5);
  });

  it("round-trips a grid through a block", () => {
    const grid = { w: 2, h: 2, tiles: [4, 5, 6, 7] };
    const created = blockFromGrid(3, "gate", grid, ["town"]);
    expect(created).toMatchObject({ id: 3, name: "gate", w: 2, h: 2, tags: ["town"] });
    expect(blockGrid(created)).toEqual(grid);
  });

  it("replaces tiles while keeping identity and tags", () => {
    const updated = withGrid(block({ tags: ["town"] }), { w: 1, h: 2, tiles: [8, 9] }, "gatehouse");
    expect(updated).toMatchObject({ id: 1, name: "gatehouse", w: 1, h: 2, tags: ["town"] });
    expect(blockGrid(updated).tiles).toEqual([8, 9]);
  });

  it("inserts new blocks and updates existing ones in place", () => {
    const first = block();
    const second = block({ id: 2, name: "well" });
    const list = upsertBlock([first], second);
    expect(list).toHaveLength(2);
    const renamed = upsertBlock(list, { ...second, name: "deep well" });
    expect(renamed).toHaveLength(2);
    expect(renamed[1]!.name).toBe("deep well");
    expect(removeBlock(renamed, 1)).toEqual([{ ...second, name: "deep well" }]);
  });

  it("duplicates under a fresh id with a distinguishable name", () => {
    const copy = duplicateBlock([block()], block());
    expect(copy).toMatchObject({ id: 2, name: "hut copy", tiles: "4,5,6,7" });
  });

  it("captures a map rectangle and clips at the map bounds", () => {
    expect(gridFromMap(map(), { x: 1, y: 1, w: 2, h: 2 })).toEqual({ w: 2, h: 2, tiles: [5, 6, 8, 9] });
    expect(gridFromMap(map(), { x: 2, y: 2, w: 4, h: 4 })).toEqual({ w: 1, h: 1, tiles: [9] });
  });

  it("converts a block into an editor clipboard payload", () => {
    expect(blockClipboard(block())).toEqual({ w: 2, h: 2, tiles: [4, 5, 6, 7] });
  });

  it("labels unnamed blocks by id", () => {
    expect(blockLabel(block({ name: "" }))).toBe("Block 1");
    expect(blockLabel(block())).toBe("hut");
  });
});
