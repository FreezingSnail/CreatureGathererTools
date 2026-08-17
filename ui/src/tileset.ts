// Tileset PNG loading + tile slicing. Bytes come from the Tauri
// `read_file_bytes` command; assets are sliced into 16x16 tile canvases.

import { TILE_SIZE } from "./geometry";

export interface Tileset {
  tiles: HTMLCanvasElement[];
  perRow: number;
}

export function emptyTileset(): Tileset {
  return { tiles: [], perRow: 0 };
}

/** Slice a fully drawn source canvas into tileSize x tileSize tiles (Go loadTileset parity). */
export function sliceCanvas(src: HTMLCanvasElement, tileSize = TILE_SIZE): Tileset {
  const perRow = Math.max(0, Math.floor(src.width / tileSize));
  const perCol = Math.max(0, Math.floor(src.height / tileSize));
  const tiles: HTMLCanvasElement[] = [];
  for (let id = 0; id < perRow * perCol; id++) {
    const tx = (id % perRow) * tileSize;
    const ty = Math.floor(id / perRow) * tileSize;
    const tile = document.createElement("canvas");
    tile.width = tileSize;
    tile.height = tileSize;
    const ctx = tile.getContext("2d");
    if (ctx) {
      ctx.drawImage(src, tx, ty, tileSize, tileSize, 0, 0, tileSize, tileSize);
    }
    tiles.push(tile);
  }
  return { tiles, perRow };
}

/** Decode PNG bytes into a tileset. Rejects on bad bytes; returns empty tileset on missing dims. */
export async function loadTilesetFromBytes(
  bytes: Uint8Array,
  tileSize = TILE_SIZE
): Promise<Tileset> {
  const blob = new Blob([bytes as unknown as BlobPart]);
  const url = URL.createObjectURL(blob);
  try {
    const img = new Image();
    await new Promise<void>((resolve, reject) => {
      img.onload = () => resolve();
      img.onerror = () => reject(new Error("failed to decode image bytes"));
      img.src = url;
    });
    const canvas = document.createElement("canvas");
    canvas.width = img.naturalWidth || img.width;
    canvas.height = img.naturalHeight || img.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return emptyTileset();
    }
    ctx.drawImage(img, 0, 0);
    return sliceCanvas(canvas, tileSize);
  } finally {
    URL.revokeObjectURL(url);
  }
}

/** Load the map tileset from its config path (with legacy tileset_png fallback, Go parity). */
export async function loadMapTileset(
  readBytes: (path: string) => Promise<Uint8Array>,
  mapTilesetPng: string,
  legacyTilesetPng: string
): Promise<Tileset> {
  const path = mapTilesetPng || legacyTilesetPng;
  if (!path) {
    return emptyTileset();
  }
  try {
    const bytes = await readBytes(path);
    return await loadTilesetFromBytes(bytes);
  } catch {
    return emptyTileset();
  }
}