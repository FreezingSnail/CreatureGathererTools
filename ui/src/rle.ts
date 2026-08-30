/** Run-length codec matching crates/core/src/wfcmap/rle.rs. */
export function encode(tiles: number[]): string {
  const runs: string[] = [];
  let index = 0;
  while (index < tiles.length) {
    const tile = tiles[index];
    let count = 1;
    while (index + count < tiles.length && tiles[index + count] === tile) {
      count += 1;
    }
    let remaining = count;
    while (remaining > 0) {
      const chunk = Math.min(remaining, 0xffffffff);
      runs.push(chunk === 1 ? String(tile) : `${tile}x${chunk}`);
      remaining -= chunk;
    }
    index += count;
  }
  return runs.join(",");
}

function malformed(text: string): Error {
  return new Error(`malformed RLE run: ${JSON.stringify(text)}`);
}

function decimal(text: string, raw: string): number {
  if (!text || !/^[0-9]+$/.test(text)) {
    throw malformed(raw);
  }
  const value = Number(text);
  if (!Number.isSafeInteger(value)) {
    throw malformed(raw);
  }
  return value;
}

export function decode(source: string, expectedLen: number): number[] {
  if (!Number.isSafeInteger(expectedLen) || expectedLen < 0) {
    throw new Error(`invalid expected RLE length: ${expectedLen}`);
  }
  if (source.trim() === "") {
    if (expectedLen !== 0) {
      throw new Error(`RLE length 0 does not match expected ${expectedLen}`);
    }
    return [];
  }

  const tiles: number[] = [];
  let offset = 0;
  for (const raw of source.split(",")) {
    const at = offset;
    offset += raw.length + 1;
    const run = raw.trim();
    if (!run) {
      throw malformed(raw);
    }
    const parts = run.split("x");
    if (parts.length > 2) {
      throw malformed(raw);
    }
    const tile = decimal(parts[0], raw);
    if (tile > 0xffff) {
      throw new Error(`RLE tile value out of u16 range at byte ${at}`);
    }
    const count = parts.length === 2 ? decimal(parts[1], raw) : 1;
    if (count === 0) {
      throw new Error(`zero RLE count at byte ${at}`);
    }
    if (tiles.length + count > expectedLen) {
      throw new Error(`RLE length ${tiles.length + count} does not match expected ${expectedLen}`);
    }
    for (let i = 0; i < count; i += 1) {
      tiles.push(tile);
    }
  }
  if (tiles.length !== expectedLen) {
    throw new Error(`RLE length ${tiles.length} does not match expected ${expectedLen}`);
  }
  return tiles;
}
