// Shared block preview. Draws a block's decoded tiles onto a canvas scaled to
// fit `max` pixels, so libraries and pickers render blocks identically.

import { useEffect, useRef } from "react";
import type { Block } from "../types";
import { blockGrid } from "../blocks";

export interface BlockThumbnailProps {
  block: Block;
  tiles: HTMLCanvasElement[];
  /** Longest edge in pixels. */
  max?: number;
}

export function BlockThumbnail({ block, tiles, max = 96 }: BlockThumbnailProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const grid = blockGrid(block);
    const scale = Math.max(1, Math.min(16, Math.floor(max / Math.max(grid.w, grid.h, 1))));
    canvas.width = Math.max(1, grid.w * scale);
    canvas.height = Math.max(1, grid.h * scale);
    context.clearRect(0, 0, canvas.width, canvas.height);
    grid.tiles.forEach((tile, index) => {
      const source = tiles[tile];
      if (!source) return;
      context.drawImage(source, (index % grid.w) * scale, Math.floor(index / grid.w) * scale, scale, scale);
    });
  }, [block, tiles, max]);
  return (
    <canvas
      ref={ref}
      aria-label={`${block.name || `Block ${block.id}`} thumbnail`}
      className="shrink-0 rounded border bg-muted/30"
      style={{ imageRendering: "pixelated" }}
    />
  );
}

export default BlockThumbnail;
