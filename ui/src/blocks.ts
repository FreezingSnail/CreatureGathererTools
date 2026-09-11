// Project block library operations. Blocks are reusable tile rectangles
// persisted in the project blocks file (crates/core/src/blocks.rs) and used
// both as a map-editor brush and as WFC piece input.

import type { Block, GameMap, TileRect } from "./types";
import { decodeGrid, encodeGrid, type TileGrid } from "./tileGrid";

export function nextBlockId(blocks: Block[]): number {
  return blocks.reduce((max, block) => Math.max(max, block.id), 0) + 1;
}

export function blockGrid(block: Block): TileGrid {
  return decodeGrid(block.tiles, block.w, block.h);
}

export function blockFromGrid(id: number, name: string, grid: TileGrid, tags: string[] = []): Block {
  return { id, name, w: grid.w, h: grid.h, tiles: encodeGrid(grid), tags };
}

/** Replace a block's tiles while keeping its identity and tags. */
export function withGrid(block: Block, grid: TileGrid, name?: string): Block {
  return { ...block, name: name ?? block.name, w: grid.w, h: grid.h, tiles: encodeGrid(grid) };
}

export function upsertBlock(blocks: Block[], block: Block): Block[] {
  return blocks.some((item) => item.id === block.id)
    ? blocks.map((item) => (item.id === block.id ? block : item))
    : [...blocks, block];
}

export function removeBlock(blocks: Block[], id: number): Block[] {
  return blocks.filter((block) => block.id !== id);
}

/** Copy a block under a fresh id, marking the name so the two are telling apart. */
export function duplicateBlock(blocks: Block[], block: Block): Block {
  return { ...block, id: nextBlockId(blocks), name: `${block.name} copy` };
}

/** Read a rectangle out of a map's tile grid, clipping at the map bounds. */
export function gridFromMap(map: GameMap, rect: TileRect): TileGrid {
  const tiles: number[] = [];
  const w = Math.max(0, Math.min(rect.w, map.width - rect.x));
  const h = Math.max(0, Math.min(rect.h, map.height - rect.y));
  for (let y = rect.y; y < rect.y + h; y += 1) {
    for (let x = rect.x; x < rect.x + w; x += 1) {
      tiles.push(map.tiles[y]?.[x] ?? 0);
    }
  }
  return { w: Math.max(1, w), h: Math.max(1, h), tiles: tiles.length ? tiles : [0] };
}

/** Clipboard payload for the map editor paste tool. */
export function blockClipboard(block: Block): { w: number; h: number; tiles: number[] } {
  const grid = blockGrid(block);
  return { w: grid.w, h: grid.h, tiles: grid.tiles };
}

export function blockLabel(block: Block): string {
  return block.name || `Block ${block.id}`;
}
