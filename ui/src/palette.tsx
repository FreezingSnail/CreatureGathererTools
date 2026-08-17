// Tile palette: a scrollable grid of tilesets. Shift-click drag-selects a
// row-major rectangle (Go buildPalette parity). Uses toDataURL so jsdom-safe.

import { useMemo, useRef } from "react";
import { selectRect } from "./geometry";

export interface PaletteProps {
  tiles: HTMLCanvasElement[];
  perRow: number;
  selected: number[];
  emptyLabel?: string;
  onSelectBlock: (block: number[], w: number, h: number) => void;
}

function TileButton({
  canvas,
  isSelected,
  onTap,
}: {
  canvas: HTMLCanvasElement;
  isSelected: boolean;
  onTap: (shift: boolean) => void;
}) {
  const url = useMemo(() => {
    try {
      return canvas.toDataURL();
    } catch {
      return "";
    }
  }, [canvas]);
  return (
    <button
      type="button"
      title={`tile ${canvas.width}x${canvas.height}`}
      onClick={(e) => onTap(e.shiftKey)}
      style={{
        width: 32,
        height: 32,
        padding: 0,
        border: isSelected ? "2px solid #ffdc00" : "1px solid #444",
        outline: isSelected ? "1px solid #000" : "none",
        backgroundImage: url ? `url(${url})` : undefined,
        backgroundSize: "cover",
        imageRendering: "pixelated",
        cursor: "pointer",
      }}
      data-testid="palette-tile"
    />
  );
}

export function Palette({
  tiles,
  perRow,
  selected,
  emptyLabel = "No tileset loaded",
  onSelectBlock,
}: PaletteProps) {
  const anchorRef = useRef(0);
  const selectedSet = useMemo(() => new Set(selected), [selected]);

  if (tiles.length === 0) {
    return (
      <div style={{ padding: 8, color: "#888" }} data-testid="palette">
        {emptyLabel}
      </div>
    );
  }

  const cols = Math.max(1, perRow);

  const tap = (idx: number, shift: boolean) => {
    if (shift) {
      const { block, w, h } = selectRect(anchorRef.current, idx, cols, tiles.length);
      onSelectBlock(block, w, h);
    } else {
      anchorRef.current = idx;
      const { block, w, h } = selectRect(idx, idx, cols, tiles.length);
      onSelectBlock(block, w, h);
    }
  };

  return (
    <div
      style={{
        overflow: "auto",
        maxHeight: 420,
        display: "grid",
        gridTemplateColumns: `repeat(${cols}, 32px)`,
        gap: 2,
        padding: 4,
      }}
      data-testid="palette"
    >
      {tiles.map((t, i) => (
        <TileButton
          key={i}
          canvas={t}
          isSelected={selectedSet.has(i)}
          onTap={(shift) => tap(i, shift)}
        />
      ))}
    </div>
  );
}