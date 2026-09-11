// Turning project blocks into WFC pieces. WFC slices pieces on the project
// slot grid, so a block is only usable when both extents are slot multiples.

import { encode } from "./rle";
import type { Block, WfcPiece } from "./types";
import { blockGrid, blockLabel } from "./blocks";
import { createGrid, encodeGrid, type TileGrid } from "./tileGrid";
import { nextWfcId, pieceTiles } from "./wfc";

/** Tag applied to pieces captured directly from the active sprite sheet. */
export const SHEET_TAG = "sheet";
/** Convert an exact sprite-sheet rectangle into a slot-aligned WFC piece. */
export function sheetSelectionToPiece(
  pieces: WfcPiece[],
  tiles: number[],
  w: number,
  h: number,
  slotW: number,
  slotH: number,
  name: string,
  weight = 1,
  limit: number | null = null,
): { piece: WfcPiece } | { error: string } {
  if (!Number.isInteger(w) || !Number.isInteger(h) || w < 1 || h < 1) return { error: "Sheet selection must be at least 1×1 tile." };
  if (!Number.isInteger(slotW) || !Number.isInteger(slotH) || slotW < 1 || slotH < 1) return { error: "WFC slot size must be positive." };
  if (tiles.length !== w * h) return { error: `Sheet selection is incomplete; expected ${w * h} tiles but received ${tiles.length}.` };
  if (tiles.some((tile) => tile === -1)) return { error: "Sheet selection is incomplete; it crosses the edge of the sprite sheet." };
  if (tiles.some((tile) => !Number.isInteger(tile) || tile < 0)) return { error: "Sheet selection contains an invalid tile." };
  if (w % slotW !== 0 || h % slotH !== 0) return { error: `Sheet selection ${w}×${h} must be exact multiples of the ${slotW}×${slotH} WFC slot.` };
  if (!Number.isFinite(weight) || weight <= 0) return { error: "Piece weight must be greater than zero." };
  return {
    piece: {
      id: nextWfcId(pieces),
      name: name.trim() || `Sheet ${nextWfcId(pieces)}`,
      tw: w,
      th: h,
      tiles: encode(tiles),
      weight,
      limit,
      edge_overrides: [],
      tags: [SHEET_TAG],
    },
  };
}
/** Tag applied to pieces created from single sheet tiles. */
export const FILL_TAG = "fill";
/** Tag applied to pieces imported from the project block library. */
export const BLOCK_TAG = "block";

export function isSlotAligned(block: Block, slotW: number, slotH: number): boolean {
  return slotW > 0 && slotH > 0 && block.w % slotW === 0 && block.h % slotH === 0;
}

export function alignmentMessage(block: Block, slotW: number, slotH: number): string | null {
  return isSlotAligned(block, slotW, slotH)
    ? null
    : `${blockLabel(block)} is ${block.w}×${block.h}; WFC needs multiples of ${slotW}×${slotH}.`;
}

function piece(id: number, name: string, grid: TileGrid, tags: string[], weight = 1): WfcPiece {
  return {
    id,
    name,
    tw: grid.w,
    th: grid.h,
    tiles: encodeGrid(grid),
    weight,
    limit: null,
    edge_overrides: [],
    tags,
  };
}

/** Convert a block into a piece, or return the reason it cannot be used. */
export function blockToPiece(
  pieces: WfcPiece[],
  block: Block,
  slotW: number,
  slotH: number,
): { piece: WfcPiece } | { error: string } {
  const error = alignmentMessage(block, slotW, slotH);
  if (error) return { error };
  return { piece: piece(nextWfcId(pieces), blockLabel(block), blockGrid(block), [BLOCK_TAG]) };
}

/** Import several blocks at once, reporting per-block reasons for skips. */
export function blocksToPieces(
  pieces: WfcPiece[],
  blocks: Block[],
  slotW: number,
  slotH: number,
): { created: WfcPiece[]; errors: string[] } {
  const created: WfcPiece[] = [];
  const errors: string[] = [];
  for (const block of blocks) {
    const result = blockToPiece([...pieces, ...created], block, slotW, slotH);
    if ("error" in result) errors.push(result.error);
    else created.push(result.piece);
  }
  return { created, errors };
}

/** The single tile a piece is filled with, or null when it is not uniform. */
export function fillTileOf(piece: WfcPiece): number | null {
  const tiles = pieceTiles(piece);
  if (!tiles.length) return null;
  return tiles.every((tile) => tile === tiles[0]) ? tiles[0]! : null;
}

export function fillPieceName(tile: number): string {
  return `Fill ${tile}`;
}

/** Sheet tiles that have no uniform fill piece yet, in ascending order. */
export function missingFillTiles(pieces: WfcPiece[], tileIds: number[]): number[] {
  const covered = new Set(pieces.flatMap((item) => {
    const tile = fillTileOf(item);
    return tile === null ? [] : [tile];
  }));
  const wanted = new Set(tileIds.filter((tile) => Number.isInteger(tile) && tile >= 0 && !covered.has(tile)));
  return [...wanted].sort((a, b) => a - b);
}

/** Slot-sized uniform pieces for each requested tile, skipping covered tiles. */
export function fillPiecesForTiles(
  pieces: WfcPiece[],
  tileIds: number[],
  slotW: number,
  slotH: number,
): WfcPiece[] {
  const created: WfcPiece[] = [];
  for (const tile of missingFillTiles(pieces, tileIds)) {
    created.push(piece(
      nextWfcId([...pieces, ...created]),
      fillPieceName(tile),
      createGrid(Math.max(1, slotW), Math.max(1, slotH), tile),
      [FILL_TAG],
    ));
  }
  return created;
}
