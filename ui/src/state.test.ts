import { describe, expect, it } from "vitest";
import {
  emptyMap,
  initialState,
  makeGrid,
  normalizeMap,
  reducer,
  MAX_UNDO,
} from "./state";
import type { GameMap } from "./types";

function paintMap() {
  let s = initialState();
  s = reducer(s, { type: "SET_MAP", map: emptyMap(4, 4) });
  s = reducer(s, { type: "PUSH_UNDO" });
  s = reducer(s, { type: "PAINT_BLOCK", col: 1, row: 1 });
  return s;
}

describe("reducer map painting", () => {
  it("paints a tile with the active block", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(4, 4) });
    s = reducer(s, { type: "SET_ACTIVE_TILE", tile: 7 });
    s = reducer(s, { type: "PAINT_BLOCK", col: 1, row: 1 });
    expect(s.map.tiles[1][1]).toBe(7);
    expect(s.map.tiles[0][0]).toBe(0);
  });

  it("paints a multi-tile block in one stroke", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "SET_ACTIVE_BLOCK", block: [1, 2, 3, 4], w: 2, h: 2 });
    s = reducer(s, { type: "PAINT_BLOCK", col: 2, row: 2 });
    expect(s.map.tiles[2][2]).toBe(1);
    expect(s.map.tiles[2][3]).toBe(2);
    expect(s.map.tiles[3][2]).toBe(3);
    expect(s.map.tiles[3][3]).toBe(4);
  });

  it("ignores out-of-bounds paints", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(4, 4) });
    s = reducer(s, { type: "SET_ACTIVE_TILE", tile: 9 });
    const before = s.map.tiles;
    s = reducer(s, { type: "PAINT_BLOCK", col: 10, row: 10 });
    expect(s.map.tiles).toEqual(before);
  });
});

describe("undo stack (max 5)", () => {
  it("undo restores the tiles snapshot", () => {
    let s = paintMap();
    expect(s.map.tiles[1][1]).toBe(0); // painted tile 0? activeTile default 0 -> paint writes 0
    // force a nonzero tile
    s = reducer(s, { type: "SET_ACTIVE_TILE", tile: 3 });
    s = reducer(s, { type: "PUSH_UNDO" });
    s = reducer(s, { type: "PAINT_BLOCK", col: 1, row: 1 });
    expect(s.map.tiles[1][1]).toBe(3);
    s = reducer(s, { type: "UNDO" });
    expect(s.map.tiles[1][1]).toBe(0);
    s = reducer(s, { type: "UNDO" });
    expect(s.undoStack.length).toBe(0);
  });

  it("caps the stack at 5 and undoes to the original", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(4, 4) });
    for (let i = 0; i < 7; i++) {
      s = reducer(s, { type: "PUSH_UNDO" });
    }
    expect(s.undoStack.length).toBe(MAX_UNDO);
    for (let i = 0; i < MAX_UNDO; i++) {
      s = reducer(s, { type: "UNDO" });
    }
    expect(s.undoStack.length).toBe(0);
    expect(s.map.tiles).toEqual(makeGrid(4, 4, 0));
  });

  it("undo on empty stack is a no-op", () => {
    const s = initialState();
    expect(reducer(s, { type: "UNDO" })).toBe(s);
  });
});

describe("npc + script objects", () => {
  it("click NPC adds then selects on repeated click", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "SET_NPC_SPRITE", sprite: 4 });
    s = reducer(s, { type: "CLICK_NPC", col: 2, row: 3 });
    expect(s.map.npcs).toHaveLength(1);
    expect(s.map.npcs[0]).toEqual({
      id: 0,
      x: 2,
      y: 3,
      sprite_id: 4,
      script: "",
    });
    expect(s.selectedNpc).toBe(0);
    s = reducer(s, { type: "CLICK_NPC", col: 2, row: 3 });
    expect(s.map.npcs).toHaveLength(1); // selected, not duplicated
  });

  it("right-click deletes an NPC on that tile (NPC layer only)", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "CLICK_NPC", col: 2, row: 3 });
    s = reducer(s, { type: "CLICK_NPC", col: 5, row: 5 });
    s = reducer(s, { type: "SET_LAYER", layer: "npc" });
    s = reducer(s, { type: "RIGHT_CLICK", col: 2, row: 3 });
    expect(s.map.npcs).toHaveLength(1);
    expect(s.map.npcs[0].x).toBe(5);
    expect(s.selectedNpc).toBe(-1);
  });

  it("click script adds, edits text, parses advisory", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "CLICK_SCRIPT", col: 1, row: 1 });
    s = reducer(s, { type: "SET_SCRIPT_TEXT", target: "script", id: 0, text: "msg {hi}" });
    expect(s.map.scripts[0].script).toBe("msg {hi}");
    s = reducer(s, { type: "SET_PARSE_RESULT", key: "script:0", result: { ok: true, errors: [], cmds: [] } });
    expect(s.parseKey).toBe("script:0");
    expect(s.parseResult?.ok).toBe(true);
  });

  it("right-click deletes scripts and clears selections", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "CLICK_SCRIPT", col: 1, row: 1 });
    s = reducer(s, { type: "SET_LAYER", layer: "script" });
    s = reducer(s, { type: "RIGHT_CLICK", col: 1, row: 1 });
    expect(s.map.scripts).toHaveLength(0);
    expect(s.selectedScript).toBe(-1);
  });
});

describe("collision + encounter layers", () => {
  it("paints collision with active type and clears on right-click", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "SET_COLLISION_TYPE", id: 3 });
    s = reducer(s, { type: "SET_BRUSH_SIZE", size: 2 });
    s = reducer(s, { type: "SET_LAYER", layer: "collision" });
    s = reducer(s, { type: "PAINT_COLLISION", col: 4, row: 4 });
    expect(s.map.collision[4][4]).toBe(3);
    expect(s.map.collision[4][3]).toBe(3); // brush reaches neighbors
    s = reducer(s, { type: "RIGHT_CLICK", col: 4, row: 4 });
    expect(s.map.collision[4][4]).toBe(0);
  });

  it("paints encounter zones and clears to -1", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "SET_ENCOUNTER_ZONE", id: 2 });
    s = reducer(s, { type: "SET_LAYER", layer: "encounter" });
    s = reducer(s, { type: "PAINT_ENCOUNTER", col: 0, row: 0 });
    expect(s.map.encounters[0][0]).toBe(2);
    s = reducer(s, { type: "RIGHT_CLICK", col: 0, row: 0 });
    expect(s.map.encounters[0][0]).toBe(-1);
  });
});

describe("view + flags", () => {
  it("clamps offset to >= 0 (Go parity)", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_OFFSET", offsetX: -5, offsetY: 12 });
    expect(s.offsetX).toBe(0);
    expect(s.offsetY).toBe(12);
  });

  it("clamps zoom between 0.5 and 8", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_ZOOM", zoom: 0.1 });
    expect(s.zoom).toBe(0.5);
    s = reducer(s, { type: "SET_ZOOM", zoom: 100 });
    expect(s.zoom).toBe(8);
  });

  it("toggles layers and overlays", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_LAYER", layer: "encounter" });
    expect(s.layer).toBe("encounter");
    s = reducer(s, { type: "TOGGLE_CHUNKS" });
    expect(s.showChunks).toBe(false);
    s = reducer(s, { type: "TOGGLE_CHUNK_HIGHLIGHT" });
    expect(s.showChunkHighlight).toBe(true);
  });
});

describe("tool switching", () => {
  it("defaults to pan and switches via SET_TOOL", () => {
    expect(initialState().tool).toBe("pan");
    let s = reducer(initialState(), { type: "SET_TOOL", tool: "paint" });
    expect(s.tool).toBe("paint");
    s = reducer(s, { type: "SET_TOOL", tool: "pan" });
    expect(s.tool).toBe("pan");
  });
});

describe("script editing open/close", () => {
  it("single click selects without opening the editor", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "CLICK_SCRIPT", col: 1, row: 1 });
    expect(s.selectedScript).toBe(0);
    expect(s.editingScript).toBe(-1);
  });

  it("OPEN_SCRIPT opens the editor and closes with id -1", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "CLICK_SCRIPT", col: 1, row: 1 });
    s = reducer(s, { type: "OPEN_SCRIPT", id: 0 });
    expect(s.editingScript).toBe(0);
    expect(s.selectedScript).toBe(0);
    s = reducer(s, { type: "OPEN_SCRIPT", id: -1 });
    expect(s.editingScript).toBe(-1);
    expect(s.selectedScript).toBe(0); // highlight stays
  });

  it("clicking another script closes the editor on the old one", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(8, 8) });
    s = reducer(s, { type: "CLICK_SCRIPT", col: 1, row: 1 });
    s = reducer(s, { type: "OPEN_SCRIPT", id: 0 });
    s = reducer(s, { type: "CLICK_SCRIPT", col: 3, row: 3 });
    expect(s.selectedScript).toBe(1);
    expect(s.editingScript).toBe(-1);
  });
});

describe("normalizeMap", () => {
  it("fills missing grids for a minimal map", () => {
    const bare = { width: 3, height: 2 } as unknown as GameMap;
    const m = normalizeMap(bare);
    expect(m.tiles).toHaveLength(2);
    expect(m.tiles[0]).toHaveLength(3);
    expect(m.collision[1][2]).toBe(0);
    expect(m.encounters[0][0]).toBe(-1);
  });

  it("keeps valid grids", () => {
    const m = normalizeMap(emptyMap(5, 5));
    expect(m.tiles).toHaveLength(5);
    expect(m.tiles[0]).toHaveLength(5);
  });
});


describe("selection and clipboard", () => {
  it("clamps selection to map bounds", () => {
    let s = initialState();
    s = reducer(s, { type: "SET_MAP", map: emptyMap(4, 3) });
    s = reducer(s, { type: "SET_SELECTION", rect: { x: -2, y: 1, w: 9, h: 9 } });
    expect(s.selection).toEqual({ x: 0, y: 1, w: 4, h: 2 });
  });

  it("copies selected tiles row-major", () => {
    let s = initialState();
    const map = emptyMap(4, 3);
    map.tiles = [[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
    s = reducer(s, { type: "SET_MAP", map });
    s = reducer(s, { type: "SET_SELECTION", rect: { x: 1, y: 0, w: 2, h: 2 } });
    s = reducer(s, { type: "COPY_SELECTION" });
    expect(s.clipboard).toEqual({ w: 2, h: 2, tiles: [2, 3, 6, 7] });
  });

  it("clips paste, pushes undo, restores, and touches only map tiles", () => {
    let s = initialState();
    const map = emptyMap(3, 3);
    map.collision[0][0] = 8;
    map.npcs = [{ id: 1, x: 1, y: 1, sprite_id: 2, script: "keep" }];
    s = reducer(s, { type: "SET_MAP", map });
    s = reducer(s, { type: "SET_CLIPBOARD", clipboard: { w: 2, h: 2, tiles: [4, 5, 6, 7] } });
    s = reducer(s, { type: "PASTE_AT", x: 2, y: 2 });
    expect(s.map.tiles[2][2]).toBe(4);
    expect(s.map.tiles[2][1]).toBe(0);
    expect(s.undoStack).toHaveLength(1);
    expect(s.map.collision[0][0]).toBe(8);
    expect(s.map.npcs[0].script).toBe("keep");
    s = reducer(s, { type: "UNDO" });
    expect(s.map.tiles).toEqual(makeGrid(3, 3, 0));
  });
});
