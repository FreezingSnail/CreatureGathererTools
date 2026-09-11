// Pure tile-grid math shared by the block builder, the map editor brush and
// WFC piece import. DOM-free so painting and resizing stay unit-testable.

import { decode, encode } from "./rle";

/** Row-major rectangle of tile ids. */
export interface TileGrid {
  w: number;
  h: number;
  tiles: number[];
}

export function createGrid(w: number, h: number, tile = 0): TileGrid {
  const width = dimension(w);
  const height = dimension(h);
  return { w: width, h: height, tiles: new Array(width * height).fill(tile) };
}

/** Edge or corner moved by a tile-cell delta. */
export type ResizeHandle = "n" | "e" | "s" | "w" | "ne" | "nw" | "se" | "sw";

export interface ResizeResult {
  grid: TileGrid;
  /** Number of discarded cells whose value was not the empty tile (0). */
  discardedNonEmpty: number;
  /** Number of discarded cells, including empty cells. */
  discardedCount: number;
}

function dimension(value: number): number {
  return Number.isFinite(value) ? Math.max(1, Math.floor(value)) : 1;
}

/** Resize keeping the top-left overlap; grown cells take `tile`. */
export function resizeGrid(grid: TileGrid, w: number, h: number, tile = 0): TileGrid {
  return resizeGridByDimensions(grid, w, h, tile).grid;
}

/** Resize from the fixed top-left origin, reporting any discarded content. */
export function resizeGridByDimensions(grid: TileGrid, w: number, h: number, tile = 0): ResizeResult {
  const next = createGrid(dimension(w), dimension(h), tile);
  let discardedCount = 0;
  let discardedNonEmpty = 0;
  for (let y = 0; y < grid.h; y += 1) {
    for (let x = 0; x < grid.w; x += 1) {
      const value = grid.tiles[y * grid.w + x] ?? tile;
      if (x < next.w && y < next.h) next.tiles[y * next.w + x] = value;
      else {
        discardedCount += 1;
        if (value !== 0) discardedNonEmpty += 1;
      }
    }
  }
  return { grid: next, discardedCount, discardedNonEmpty };
}

/**
 * Resize around the opposite fixed edge. Pointer deltas are signed cell
 * movement: moving west/north decreases the extent and translates content.
 */
export function resizeGridFromHandle(
  source: TileGrid,
  handle: ResizeHandle,
  deltaX: number,
  deltaY: number,
  tile = 0,
): ResizeResult {
  const dx = Number.isFinite(deltaX) ? Math.trunc(deltaX) : 0;
  const dy = Number.isFinite(deltaY) ? Math.trunc(deltaY) : 0;
  const movesWest = handle.includes("w");
  const movesNorth = handle.includes("n");
  const nextW = dimension(source.w + (movesWest ? -dx : handle.includes("e") ? dx : 0));
  const nextH = dimension(source.h + (movesNorth ? -dy : handle.includes("s") ? dy : 0));
  const offsetX = movesWest ? source.w - nextW : 0;
  const offsetY = movesNorth ? source.h - nextH : 0;
  const next = createGrid(nextW, nextH, tile);
  let discardedCount = 0;
  let discardedNonEmpty = 0;
  for (let y = 0; y < source.h; y += 1) {
    for (let x = 0; x < source.w; x += 1) {
      const value = source.tiles[y * source.w + x] ?? tile;
      const nx = x - offsetX;
      const ny = y - offsetY;
      if (nx >= 0 && ny >= 0 && nx < next.w && ny < next.h) next.tiles[ny * next.w + nx] = value;
      else {
        discardedCount += 1;
        if (value !== 0) discardedNonEmpty += 1;
      }
    }
  }
  return { grid: next, discardedCount, discardedNonEmpty };
}

export const hasNonEmptyDiscard = (result: ResizeResult): boolean => result.discardedNonEmpty > 0;

/** Stamp a brush block with its top-left at (x, y), clipping at the edges. */
export function stampGrid(
  grid: TileGrid,
  x: number,
  y: number,
  brush: number[],
  brushW: number,
  brushH: number,
): TileGrid {
  if (brushW <= 0 || brushH <= 0) return grid;
  const tiles = grid.tiles.slice();
  for (let row = 0; row < brushH; row += 1) {
    for (let col = 0; col < brushW; col += 1) {
      const tx = x + col;
      const ty = y + row;
      if (tx < 0 || ty < 0 || tx >= grid.w || ty >= grid.h) continue;
      const value = brush[row * brushW + col];
      if (value === undefined) continue;
      tiles[ty * grid.w + tx] = value;
    }
  }
  return { ...grid, tiles };
}

export function fillGrid(grid: TileGrid, tile: number): TileGrid {
  return { ...grid, tiles: new Array(grid.w * grid.h).fill(tile) };
}

/** Decode an RLE payload into a grid, falling back to an empty grid when the
 * payload does not match the declared extent. */
export function decodeGrid(tiles: string, w: number, h: number): TileGrid {
  const width = Math.max(1, w);
  const height = Math.max(1, h);
  try {
    return { w: width, h: height, tiles: decode(tiles, width * height) };
  } catch {
    return createGrid(width, height);
  }
}

export function encodeGrid(grid: TileGrid): string {
  return encode(grid.tiles);
}
