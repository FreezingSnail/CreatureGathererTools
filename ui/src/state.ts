// Pure editor state reducer: map data + tool settings + undo stack.
// Mirrors the Go MapCanvas/MapEditor behavior (parity port, not redesign).

import type { GameMap, ParseResult, CompileResult, TileRect } from "./types";
import {
  brushCells,
  blockCells,
  clampOffset,
  clampRect,
  clampZoom,
} from "./geometry";

export type LayerType = "map" | "npc" | "script" | "collision" | "encounter";

/** Canvas interaction tools. */
export type EditorTool = "paint" | "pan" | "select" | "paste";

export interface TileClipboard {
  w: number;
  h: number;
  tiles: number[];
}

export const LAYERS: LayerType[] = [
  "map",
  "npc",
  "script",
  "collision",
  "encounter",
];

export const MAX_UNDO = 5;

const EMPTY_SELECTION = -1;

export interface EditorState {
  map: GameMap;
  // tool settings
  tool: EditorTool;
  layer: LayerType;
  activeTile: number;
  activeBlock: number[];
  blockW: number;
  blockH: number;
  activeNpcSprite: number;
  brushSize: number;
  activeCollisionType: number;
  activeEncounterZone: number;
  // selections
  selectedNpc: number;
  selectedScript: number;
  selection: TileRect | null;
  clipboard: TileClipboard | null;
  /** Script layer: id currently open in the inline editor (-1 = closed). */
  editingScript: number;
  // view
  zoom: number;
  offsetX: number;
  offsetY: number;
  showChunks: boolean;
  showChunkHighlight: boolean;
  showCollision: boolean;
  showEncounters: boolean;
  showNpcOverlay: boolean;
  showScriptOverlay: boolean;
  // undo (snapshots of tiles only, Go parity)
  undoStack: number[][][];
  // advisory script parse state (key identifies which object it belongs to)
  parseKey: string | null;
  parseResult: ParseResult | null;
  // save / compile feedback
  status: string;
  compileResult: CompileResult | null;
}

export function makeGrid(width: number, height: number, fill: number): number[][] {
  const rows: number[][] = [];
  for (let r = 0; r < height; r++) {
    rows.push(new Array<number>(width).fill(fill));
  }
  return rows;
}

/** Ensure a loaded map has width/height and all grids sized (Go NewMapEditor parity). */
export function normalizeMap(map: GameMap): GameMap {
  const width = map.width || 256;
  const height = map.height || 256;
  const tiles =
    Array.isArray(map.tiles) &&
    map.tiles.length === height &&
    map.tiles.every((r) => r.length === width)
      ? map.tiles
      : makeGrid(width, height, 0);
  const collision =
    Array.isArray(map.collision) &&
    map.collision.length === height &&
    map.collision.every((r) => r.length === width)
      ? map.collision
      : makeGrid(width, height, 0);
  const encounters =
    Array.isArray(map.encounters) &&
    map.encounters.length === height &&
    map.encounters.every((r) => r.length === width)
      ? map.encounters
      : makeGrid(width, height, -1);
  return { ...map, width, height, tiles, collision, encounters };
}

export function emptyMap(width = 256, height = 256): GameMap {
  return normalizeMap({
    width,
    height,
    tiles: [],
    npcs: [],
    scripts: [],
    collision: [],
    collision_types: [],
    encounters: [],
    encounter_zones: [],
  });
}

export function initialState(): EditorState {
  return {
    map: emptyMap(),
    tool: "pan",
    layer: "map",
    activeTile: 0,
    activeBlock: [0],
    blockW: 1,
    blockH: 1,
    activeNpcSprite: 0,
    brushSize: 1,
    activeCollisionType: 1,
    activeEncounterZone: 0,
    selectedNpc: EMPTY_SELECTION,
    selectedScript: EMPTY_SELECTION,
    selection: null,
    clipboard: null,
    editingScript: EMPTY_SELECTION,
    zoom: 2,
    offsetX: 0,
    offsetY: 0,
    showChunks: true,
    showChunkHighlight: false,
    showCollision: true,
    showEncounters: true,
    showNpcOverlay: true,
    showScriptOverlay: true,
    undoStack: [],
    parseKey: null,
    parseResult: null,
    status: "",
    compileResult: null,
  };
}

export type Action =
  | { type: "SET_MAP"; map: GameMap }
  | { type: "UPDATE_MAP"; map: GameMap }
  | { type: "SET_TOOL"; tool: EditorTool }
  | { type: "SET_SELECTION"; rect: TileRect | null }
  | { type: "CLEAR_SELECTION" }
  | { type: "COPY_SELECTION" }
  | { type: "SET_CLIPBOARD"; clipboard: TileClipboard | null }
  | { type: "PASTE_AT"; x: number; y: number }
  | { type: "SET_LAYER"; layer: LayerType }
  | { type: "SET_ACTIVE_TILE"; tile: number }
  | { type: "SET_ACTIVE_BLOCK"; block: number[]; w: number; h: number }
  | { type: "SET_NPC_SPRITE"; sprite: number }
  | { type: "SET_BRUSH_SIZE"; size: number }
  | { type: "SET_COLLISION_TYPE"; id: number }
  | { type: "SET_ENCOUNTER_ZONE"; id: number }
  | { type: "SET_ZOOM"; zoom: number }
  | { type: "SET_OFFSET"; offsetX: number; offsetY: number }
  | { type: "TOGGLE_CHUNKS" }
  | { type: "TOGGLE_CHUNK_HIGHLIGHT" }
  | { type: "TOGGLE_COLLISION" }
  | { type: "TOGGLE_ENCOUNTERS" }
  | { type: "TOGGLE_NPC_OVERLAY" }
  | { type: "TOGGLE_SCRIPT_OVERLAY" }
  | { type: "PUSH_UNDO" }
  | { type: "UNDO" }
  | { type: "PAINT_BLOCK"; col: number; row: number }
  | { type: "PAINT_COLLISION"; col: number; row: number }
  | { type: "PAINT_ENCOUNTER"; col: number; row: number }
  | { type: "CLICK_NPC"; col: number; row: number }
  | { type: "CLICK_SCRIPT"; col: number; row: number }
  | { type: "OPEN_SCRIPT"; id: number }
  | { type: "RIGHT_CLICK"; col: number; row: number }
  | { type: "SET_SCRIPT_TEXT"; target: "npc" | "script"; id: number; text: string }
  | { type: "SET_PARSE_RESULT"; key: string; result: ParseResult }
  | { type: "SET_STATUS"; status: string }
  | { type: "SET_COMPILE_RESULT"; result: CompileResult | null };

function inBounds(map: GameMap, row: number, col: number): boolean {
  return (
    row >= 0 &&
    row < map.height &&
    col >= 0 &&
    col < map.width
  );
}

function pushUndo(state: EditorState): EditorState {
  const snap = state.map.tiles.map((row) => [...row]);
  const undoStack = [...state.undoStack, snap];
  if (undoStack.length > MAX_UNDO) {
    undoStack.splice(0, undoStack.length - MAX_UNDO);
  }
  return { ...state, undoStack };
}

export function reducer(state: EditorState, action: Action): EditorState {
  switch (action.type) {
    case "SET_MAP":
      return { ...initialState(), map: normalizeMap(action.map) };

    case "UPDATE_MAP":
      return { ...state, map: normalizeMap(action.map) };

    case "SET_TOOL":
      return { ...state, tool: action.tool };

    case "SET_SELECTION": {
      if (!action.rect) {
        return { ...state, selection: null };
      }
      return {
        ...state,
        selection: clampRect(action.rect, state.map.width, state.map.height),
      };
    }

    case "CLEAR_SELECTION":
      return { ...state, selection: null };

    case "SET_CLIPBOARD":
      return { ...state, clipboard: action.clipboard };

    case "COPY_SELECTION": {
      const rect = state.selection;
      if (!rect) {
        return state;
      }
      const tiles: number[] = [];
      for (let row = rect.y; row < rect.y + rect.h; row += 1) {
        for (let col = rect.x; col < rect.x + rect.w; col += 1) {
          tiles.push(state.map.tiles[row]?.[col] ?? 0);
        }
      }
      return { ...state, clipboard: { w: rect.w, h: rect.h, tiles } };
    }

    case "PASTE_AT": {
      const clip = state.clipboard;
      if (!clip || clip.w <= 0 || clip.h <= 0 || clip.tiles.length < clip.w * clip.h) {
        return state;
      }
      const tiles = state.map.tiles.map((row) => [...row]);
      let touched = false;
      for (let row = 0; row < clip.h; row += 1) {
        for (let col = 0; col < clip.w; col += 1) {
          const mapCol = action.x + col;
          const mapRow = action.y + row;
          if (!inBounds(state.map, mapRow, mapCol)) {
            continue;
          }
          touched = true;
          const value = clip.tiles[row * clip.w + col];
          tiles[mapRow][mapCol] = value;
        }
      }
      if (!touched) {
        return state;
      }
      return { ...pushUndo(state), map: { ...state.map, tiles } };
    }

    case "SET_LAYER":
      return { ...state, layer: action.layer };

    case "SET_ACTIVE_TILE":
      return {
        ...state,
        activeTile: action.tile,
        activeBlock: [action.tile],
        blockW: 1,
        blockH: 1,
      };

    case "SET_ACTIVE_BLOCK":
      return {
        ...state,
        activeBlock: action.block.filter((v) => v >= 0),
        blockW: action.w,
        blockH: action.h,
        activeTile: action.block[0] >= 0 ? action.block[0] : state.activeTile,
      };

    case "SET_NPC_SPRITE":
      return { ...state, activeNpcSprite: action.sprite };

    case "SET_BRUSH_SIZE":
      return { ...state, brushSize: Math.max(1, action.size) };

    case "SET_COLLISION_TYPE":
      return { ...state, activeCollisionType: action.id };

    case "SET_ENCOUNTER_ZONE":
      return { ...state, activeEncounterZone: action.id };

    case "SET_ZOOM":
      return { ...state, zoom: clampZoom(action.zoom) };

    case "SET_OFFSET": {
      const { col, row } = clampOffset(action.offsetX, action.offsetY);
      return { ...state, offsetX: col, offsetY: row };
    }

    case "TOGGLE_CHUNKS":
      return { ...state, showChunks: !state.showChunks };

    case "TOGGLE_CHUNK_HIGHLIGHT":
      return { ...state, showChunkHighlight: !state.showChunkHighlight };

    case "TOGGLE_COLLISION":
      return { ...state, showCollision: !state.showCollision };

    case "TOGGLE_ENCOUNTERS":
      return { ...state, showEncounters: !state.showEncounters };

    case "TOGGLE_NPC_OVERLAY":
      return { ...state, showNpcOverlay: !state.showNpcOverlay };

    case "TOGGLE_SCRIPT_OVERLAY":
      return { ...state, showScriptOverlay: !state.showScriptOverlay };

    case "PUSH_UNDO":
      return pushUndo(state);

    case "UNDO": {
      if (state.undoStack.length === 0) {
        return state;
      }
      const undoStack = [...state.undoStack];
      const top = undoStack.pop()!;
      return { ...state, undoStack, map: { ...state.map, tiles: top } };
    }

    case "PAINT_BLOCK": {
      const { col, row } = action;
      const cells = blockCells(col, row, state.blockW, state.blockH);
      const tiles = state.map.tiles.map((r) => [...r]);
      let painted = false;
      for (let i = 0; i < cells.length; i++) {
        const c = cells[i];
        if (!inBounds(state.map, c.row, c.col)) {
          continue;
        }
        const tileID =
          state.activeBlock[i % state.activeBlock.length] ?? state.activeTile;
        tiles[c.row][c.col] = tileID;
        painted = true;
      }
      if (!painted) {
        return state;
      }
      return { ...state, map: { ...state.map, tiles } };
    }

    case "PAINT_COLLISION": {
      const tiles = brushCells(action.col, action.row, state.brushSize);
      const collision = state.map.collision.map((r) => [...r]);
      for (const c of tiles) {
        if (!inBounds(state.map, c.row, c.col)) {
          continue;
        }
        collision[c.row][c.col] = state.activeCollisionType;
      }
      return { ...state, map: { ...state.map, collision } };
    }

    case "PAINT_ENCOUNTER": {
      const tiles = brushCells(action.col, action.row, state.brushSize);
      const encounters = state.map.encounters.map((r) => [...r]);
      for (const c of tiles) {
        if (!inBounds(state.map, c.row, c.col)) {
          continue;
        }
        encounters[c.row][c.col] = state.activeEncounterZone;
      }
      return { ...state, map: { ...state.map, encounters } };
    }

    case "CLICK_NPC": {
      const idx = state.map.npcs.findIndex(
        (n) => n.x === action.col && n.y === action.row
      );
      if (idx >= 0) {
        return { ...state, selectedNpc: idx };
      }
      const npcs = [
        ...state.map.npcs,
        {
          id: state.map.npcs.length,
          x: action.col,
          y: action.row,
          sprite_id: state.activeNpcSprite,
          script: "",
        },
      ];
      return { ...state, map: { ...state.map, npcs }, selectedNpc: npcs.length - 1 };
    }

    case "CLICK_SCRIPT": {
      const idx = state.map.scripts.findIndex(
        (s) => s.x === action.col && s.y === action.row
      );
      if (idx >= 0) {
        // single click selects only; editor opens on double-click
        return { ...state, selectedScript: idx, editingScript: EMPTY_SELECTION };
      }
      const scripts = [
        ...state.map.scripts,
        { id: state.map.scripts.length, x: action.col, y: action.row, script: "" },
      ];
      return {
        ...state,
        map: { ...state.map, scripts },
        selectedScript: scripts.length - 1,
        editingScript: EMPTY_SELECTION,
      };
    }

    case "OPEN_SCRIPT": {
      if (action.id < 0) {
        return { ...state, editingScript: EMPTY_SELECTION };
      }
      if (!state.map.scripts[action.id]) {
        return state;
      }
      return { ...state, selectedScript: action.id, editingScript: action.id };
    }

    case "RIGHT_CLICK": {
      const { col, row } = action;
      switch (state.layer) {
        case "npc": {
          const npcs = state.map.npcs.filter(
            (n) => !(n.x === col && n.y === row)
          );
          if (npcs.length === state.map.npcs.length) {
            return state;
          }
          return { ...state, map: { ...state.map, npcs }, selectedNpc: EMPTY_SELECTION };
        }
        case "script": {
          const scripts = state.map.scripts.filter(
            (s) => !(s.x === col && s.y === row)
          );
          if (scripts.length === state.map.scripts.length) {
            return state;
          }
          return {
            ...state,
            map: { ...state.map, scripts },
            selectedScript: EMPTY_SELECTION,
            editingScript: EMPTY_SELECTION,
          };
        }
        case "collision": {
          const cells = brushCells(col, row, state.brushSize);
          const collision = state.map.collision.map((r) => [...r]);
          for (const c of cells) {
            if (inBounds(state.map, c.row, c.col)) {
              collision[c.row][c.col] = 0;
            }
          }
          return { ...state, map: { ...state.map, collision } };
        }
        case "encounter": {
          const cells = brushCells(col, row, state.brushSize);
          const encounters = state.map.encounters.map((r) => [...r]);
          for (const c of cells) {
            if (inBounds(state.map, c.row, c.col)) {
              encounters[c.row][c.col] = -1;
            }
          }
          return { ...state, map: { ...state.map, encounters } };
        }
        default:
          return state;
      }
    }

    case "SET_SCRIPT_TEXT": {
      if (action.target === "npc") {
        const npcs = state.map.npcs.map((n) =>
          n.id === action.id ? { ...n, script: action.text } : n
        );
        return { ...state, map: { ...state.map, npcs } };
      }
      const scripts = state.map.scripts.map((s) =>
        s.id === action.id ? { ...s, script: action.text } : s
      );
      return { ...state, map: { ...state.map, scripts } };
    }

    case "SET_PARSE_RESULT":
      return { ...state, parseKey: action.key, parseResult: action.result };

    case "SET_STATUS":
      return { ...state, status: action.status };

    case "SET_COMPILE_RESULT":
      return { ...state, compileResult: action.result };

    default:
      return state;
  }
}