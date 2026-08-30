// Pure viewport/chunk math shared by the canvas and palette. No React here —
// unit-tested directly.

export const TILE_SIZE = 16;
export const MARGIN_X = 28; // left margin for row labels
export const MARGIN_Y = 20; // top margin for column labels

export const CHUNK_W = 8;
export const CHUNK_H = 4;
// Chunk grid for the standard 256x256 map: 32 cols x 64 rows = 2048 chunks.
export const CHUNK_COLS = 32;
export const CHUNK_ROWS = 64;

export interface TilePos {
  col: number;
  row: number;
}

/** Screen pixel -> map tile coordinate for a given zoom/offset. */
export function screenToTile(
  px: number,
  py: number,
  offsetX: number,
  offsetY: number,
  zoom: number
): TilePos {
  const tile = TILE_SIZE * zoom;
  return {
    col: Math.floor((px - MARGIN_X) / tile) + offsetX,
    row: Math.floor((py - MARGIN_Y) / tile) + offsetY,
  };
}

/** Map tile coordinate -> screen pixel (top-left of the tile). */
export function tileToPixel(
  col: number,
  row: number,
  offsetX: number,
  offsetY: number,
  zoom: number
): { px: number; py: number } {
  const tile = TILE_SIZE * zoom;
  return {
    px: MARGIN_X + (col - offsetX) * tile,
    py: MARGIN_Y + (row - offsetY) * tile,
  };
}

/** Inclusive drag endpoints -> a positive row-major tile rectangle. */
export function rectFromPoints(a: TilePos, b: TilePos): { x: number; y: number; w: number; h: number } {
  const x = Math.min(a.col, b.col);
  const y = Math.min(a.row, b.row);
  return { x, y, w: Math.abs(a.col - b.col) + 1, h: Math.abs(a.row - b.row) + 1 };
}

/** Clamp a rectangle to a map, preserving a non-empty intersection. */
export function clampRect(
  rect: { x: number; y: number; w: number; h: number },
  width: number,
  height: number
): { x: number; y: number; w: number; h: number } | null {
  const x = Math.max(0, Math.min(width, rect.x));
  const y = Math.max(0, Math.min(height, rect.y));
  const endX = Math.max(x, Math.min(width, rect.x + rect.w));
  const endY = Math.max(y, Math.min(height, rect.y + rect.h));
  return endX > x && endY > y ? { x, y, w: endX - x, h: endY - y } : null;
}

/** Go parity: pan offset clamps to >= 0. */
export function clampOffset(offsetX: number, offsetY: number): TilePos {
  return { col: Math.max(0, offsetX), row: Math.max(0, offsetY) };
}

export function clampZoom(zoom: number): number {
  return Math.min(8, Math.max(0.5, zoom));
}

/** Brush cells centered on (col,row), Go paintBrush parity. */
export function brushCells(
  col: number,
  row: number,
  brushSize: number
): TilePos[] {
  const r = Math.floor(brushSize / 2);
  const cells: TilePos[] = [];
  for (let dr = -r; dr <= r; dr++) {
    for (let dc = -r; dc <= r; dc++) {
      cells.push({ col: col + dc, row: row + dr });
    }
  }
  return cells;
}

/** Block cells for a row-major WxH block stamped at (col,row). */
export function blockCells(
  col: number,
  row: number,
  blockW: number,
  blockH: number
): TilePos[] {
  const cells: TilePos[] = [];
  for (let dr = 0; dr < blockH; dr++) {
    for (let dc = 0; dc < blockW; dc++) {
      cells.push({ col: col + dc, row: row + dr });
    }
  }
  return cells;
}

/** 1D chunk index for a tile, Go chunkId parity (256-map layout). */
export function chunkId(col: number, row: number): number {
  return Math.floor(row / CHUNK_H) * CHUNK_COLS + Math.floor(col / CHUNK_W);
}

/** Rectangle select helper for the palette: (a,b) indices in a cols-wide grid -> row-major block. */
export function selectRect(
  a: number,
  b: number,
  cols: number,
  total: number
): { block: number[]; w: number; h: number } {
  if (cols <= 0) {
    return { block: [], w: 0, h: 0 };
  }
  const aRow = Math.floor(a / cols);
  const aCol = a % cols;
  const bRow = Math.floor(b / cols);
  const bCol = b % cols;
  const r0 = Math.min(aRow, bRow);
  const r1 = Math.max(aRow, bRow);
  const c0 = Math.min(aCol, bCol);
  const c1 = Math.max(aCol, bCol);
  const w = c1 - c0 + 1;
  const h = r1 - r0 + 1;
  const block: number[] = [];
  for (let dr = 0; dr < h; dr++) {
    for (let dc = 0; dc < w; dc++) {
      const idx = (r0 + dr) * cols + (c0 + dc);
      block.push(idx < total ? idx : -1);
    }
  }
  return { block, w, h };
}

/** "#RRGGBBAA" / "#RRGGBB" -> [r,g,b,a]. Falls back to gray 128 alpha. */
export function parseHexColor(s: string): [number, number, number, number] {
  let hex = s;
  if (hex.startsWith("#")) {
    hex = hex.slice(1);
  }
  const parse = (i: number) => parseInt(hex.slice(i, i + 2), 16);
  if (hex.length >= 8) {
    return [parse(0), parse(2), parse(4), parse(6)];
  }
  if (hex.length >= 6) {
    return [parse(0), parse(2), parse(4), 128];
  }
  return [200, 200, 200, 128];
}

const ZONE_COLORS: [number, number, number, number][] = [
  [100, 220, 100, 100],
  [100, 100, 220, 100],
  [220, 180, 50, 100],
  [220, 80, 180, 100],
  [80, 220, 220, 100],
  [220, 120, 80, 100],
];

export function zoneColor(id: number): [number, number, number, number] {
  if (id < 0) {
    return [0, 0, 0, 0];
  }
  return ZONE_COLORS[id % ZONE_COLORS.length];
}