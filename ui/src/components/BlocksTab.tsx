// Blocks tab: the single place blocks are authored. A block is painted from
// the project sprite sheet and saved to the project block library, which the
// map editor uses as a paste brush and the WFC tab imports as pieces.

import { useEffect, useMemo, useRef, useState } from "react";
import type { Block } from "../types";
import { Palette } from "../palette";
import { blockFromGrid, blockGrid, blockLabel, duplicateBlock, nextBlockId, removeBlock, upsertBlock, withGrid } from "../blocks";
import { createGrid, fillGrid, resizeGridByDimensions, stampGrid, type ResizeResult, type TileGrid } from "../tileGrid";
import { Button } from "./ui/button";
import { BlockResizeHandles } from "./BlockResizeHandles";
import { Input } from "./ui/input";
import { BlockThumbnail } from "./BlockThumbnail";

const CELL_PX = 22;

export interface BlocksTabProps {
  blocks: Block[];
  tiles: HTMLCanvasElement[];
  perRow: number;
  /** Persisted library changes; the shell writes them to the project file. */
  onBlocksChange: (blocks: Block[]) => void;
  saveState?: "idle" | "saving" | "saved" | "error";
  saveError?: string | null;
}

interface Brush {
  tiles: number[];
  w: number;
  h: number;
}

function resizeMessage(result: ResizeResult): string | null {
  return result.discardedNonEmpty > 0
    ? `Shrink block to ${result.grid.w}×${result.grid.h}? This discards ${result.discardedNonEmpty} non-empty tile(s).`
    : null;
}

export function BlocksTab({ blocks, tiles, perRow, onBlocksChange, saveState = "idle", saveError }: BlocksTabProps) {
  const [grid, setGrid] = useState<TileGrid>(() => createGrid(1, 1));
  const [preview, setPreview] = useState<ResizeResult | null>(null);
  const [brush, setBrush] = useState<Brush>({ tiles: [0], w: 1, h: 1 });
  const [name, setName] = useState("");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const painting = useRef(false);

  const tileUrls = useMemo(() => tiles.map((tile) => {
    try {
      return tile.toDataURL();
    } catch {
      return "";
    }
  }), [tiles]);

  useEffect(() => {
    const stop = () => { painting.current = false; };
    window.addEventListener("pointerup", stop);
    return () => window.removeEventListener("pointerup", stop);
  }, []);

  const editing = blocks.find((block) => block.id === editingId) ?? null;
  const displayGrid = preview?.grid ?? grid;

  const commitResize = (result: ResizeResult) => {
    if (result.grid.w === grid.w && result.grid.h === grid.h) return;
    const warning = resizeMessage(result);
    if (warning && !window.confirm(warning)) return;
    setGrid(result.grid);
    setPreview(null);
    setStatus(warning ? `Resized to ${result.grid.w}×${result.grid.h}.` : null);
  };

  const resizeByDimensions = (w: number, h: number) => {
    commitResize(resizeGridByDimensions(grid, w, h));
  };

  const paint = (x: number, y: number) => {
    if (preview) return;
    setGrid((current) => stampGrid(current, x, y, brush.tiles, brush.w, brush.h));
  };

  const createFromSelection = () => {
    setGrid({ w: brush.w, h: brush.h, tiles: brush.tiles.slice() });
    setPreview(null);
    setStatus(`New block created from selected ${brush.w}×${brush.h} rectangle.`);
  };

  const startNew = () => {
    setEditingId(null);
    setGrid(createGrid(1, 1));
    setPreview(null);
    setName("");
    setStatus(null);
  };

  const edit = (block: Block) => {
    setEditingId(block.id);
    setGrid(blockGrid(block));
    setPreview(null);
    setName(block.name);
    setStatus(null);
  };

  const save = () => {
    if (editing) {
      onBlocksChange(upsertBlock(blocks, withGrid(editing, grid, name.trim() || editing.name)));
      setStatus(`Saved ${blockLabel(editing)}.`);
      return;
    }
    const id = nextBlockId(blocks);
    const block = blockFromGrid(id, name.trim() || `Block ${id}`, grid);
    onBlocksChange(upsertBlock(blocks, block));
    setEditingId(id);
    setStatus(`Saved ${blockLabel(block)}.`);
  };

  const duplicate = (block: Block) => {
    const copy = duplicateBlock(blocks, block);
    onBlocksChange(upsertBlock(blocks, copy));
    setStatus(`Duplicated ${blockLabel(block)}.`);
  };

  const remove = (block: Block) => {
    if (!window.confirm(`Delete ${blockLabel(block)}? WFC pieces already imported from it are unaffected.`)) return;
    onBlocksChange(removeBlock(blocks, block.id));
    if (editingId === block.id) startNew();
    setStatus(`Deleted ${blockLabel(block)}.`);
  };

  const rename = (block: Block) => {
    const next = window.prompt("Block name", block.name)?.trim();
    if (!next || next === block.name) return;
    onBlocksChange(upsertBlock(blocks, { ...block, name: next }));
    if (editingId === block.id) setName(next);
  };

  const saveLabel = saveState === "saving" ? "Saving to project…"
    : saveState === "error" ? `Save failed: ${saveError ?? "unknown error"}`
    : saveState === "saved" ? "Saved to project blocks file"
    : null;

  return (
    <div data-testid="blocks-tab" className="flex h-full min-h-0 flex-col gap-3 overflow-auto p-3 text-sm">
      <div className="flex flex-wrap items-baseline justify-between gap-2 border-b pb-3">
        <div>
          <strong className="text-base">Blocks</strong>
          <p className="text-xs text-muted-foreground">
            Paint reusable tile blocks from the project sprite sheet. Blocks are saved in the project and can be used as a map brush or imported as WFC pieces.
          </p>
        </div>
        {saveLabel && (
          <span role="status" className={`text-xs ${saveState === "error" ? "text-destructive" : "text-muted-foreground"}`}>
            {saveLabel}
          </span>
        )}
      </div>

      <div className="grid min-h-0 gap-3 xl:grid-cols-[minmax(220px,1fr)_minmax(0,1.2fr)_minmax(240px,1fr)]">
        <section aria-label="Sprite sheet" className="min-w-0">
          <div className="mb-1 text-xs font-medium">Sprite sheet</div>
          <div className="max-h-[60vh] overflow-auto rounded border bg-muted/20">
            <Palette
              tiles={tiles}
              perRow={Math.max(1, perRow)}
              selected={brush.tiles}
              tilePx={28}
              emptyLabel="No project sprite sheet loaded"
              testId="block-sheet"
              onSelectBlock={(block, w, h) => {
                if (w < 1 || h < 1 || block.length !== w * h || block.some((tile) => tile < 0)) {
                  setStatus("Selection is incomplete; choose a rectangle wholly inside the sprite sheet.");
                  return;
                }
                setBrush({ tiles: block, w, h });
                setStatus(`Selected ${w}×${h} brush.`);
              }}
            />
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Click a tile, Shift-click for an exact rectangle. Palette clicks only change the brush; use Create from selection to start a new block from it. Editing preserves the current canvas. Brush {brush.w}×{brush.h}.</p>
        </section>

        <section aria-label="Block canvas" className="min-w-0">
          <div className="mb-1 flex flex-wrap items-center gap-2 text-xs">
            <span className="font-medium">{editing ? `Editing ${blockLabel(editing)}` : "New block"}</span>
            <label className="flex items-center gap-1">W
              <Input aria-label="Block width" className="w-16" type="number" min="1" value={grid.w}
                onChange={(event) => resizeByDimensions(Number(event.target.value), grid.h)} />
            </label>
            <label className="flex items-center gap-1">H
              <Input aria-label="Block height" className="w-16" type="number" min="1" value={grid.h}
                onChange={(event) => resizeByDimensions(grid.w, Number(event.target.value))} />
            </label>
            <Button type="button" size="xs" variant="outline" onClick={() => setGrid((current) => fillGrid(current, brush.tiles[0] ?? 0))}>Flood</Button>
            <Button type="button" size="xs" variant="outline" onClick={() => setGrid((current) => fillGrid(current, 0))}>Clear</Button>
          </div>
          <p id="block-resize-instructions" className="mb-1 text-xs text-muted-foreground">Drag any edge or corner to resize, snapped to tiles. Focus a handle and use arrow keys for one-tile precision. Escape cancels a drag.</p>
          <div className="overflow-auto rounded border bg-muted/20 p-3">
            <div className="relative inline-block">
              <div className="inline-grid" style={{ gridTemplateColumns: `repeat(${displayGrid.w}, ${CELL_PX}px)`, gap: 1, opacity: preview ? 0.7 : 1 }} data-testid="block-canvas">
                {displayGrid.tiles.map((tile, index) => {
                  const x = index % displayGrid.w;
                  const y = Math.floor(index / displayGrid.w);
                  return (
                    <button
                      key={index}
                      type="button"
                      aria-label={`Block cell ${x},${y} tile ${tile}`}
                      data-testid="block-cell"
                      className="aspect-square border border-neutral-700"
                      style={{
                        backgroundImage: tileUrls[tile] ? `url(${tileUrls[tile]})` : undefined,
                        backgroundSize: "cover",
                        imageRendering: "pixelated",
                      }}
                      onPointerDown={() => { if (!preview) { painting.current = true; paint(x, y); } }}
                      onPointerEnter={() => { if (painting.current && !preview) paint(x, y); }}
                      onClick={() => paint(x, y)}
                    />
                  );
                })}
              </div>
              <BlockResizeHandles grid={grid} cellPx={CELL_PX} onPreview={setPreview} onCommit={commitResize} />
            </div>
          </div>
          {preview && <p role="status" aria-live="polite" className="mt-1 text-xs text-muted-foreground">Pending size {preview.grid.w}×{preview.grid.h}{preview.discardedNonEmpty ? `; ${preview.discardedNonEmpty} non-empty tile(s) would be discarded.` : "."}</p>}
          <div className="mt-2 flex flex-wrap items-end gap-2">
            <label className="grid gap-1 text-xs">Block name
              <Input aria-label="Block name" value={name} placeholder="Optional name" onChange={(event) => setName(event.target.value)} />
            </label>
            {!editing && <Button type="button" variant="outline" onClick={createFromSelection}>Create from selection</Button>}
            <Button type="button" onClick={save}>{editing ? "Save block" : "Add block"}</Button>
            {editing && <Button type="button" variant="outline" onClick={startNew}>New block</Button>}
          </div>
          {status && <p role="status" className="mt-2 text-xs text-muted-foreground">{status}</p>}
        </section>

        <section aria-label="Block library" className="min-w-0">
          <div className="mb-1 flex items-baseline justify-between gap-2 text-xs">
            <span className="font-medium">Library</span>
            <span className="text-muted-foreground">{blocks.length} block{blocks.length === 1 ? "" : "s"}</span>
          </div>
          {!blocks.length && <p className="rounded border border-dashed p-3 text-xs text-muted-foreground">No blocks yet. Paint one and press Add block.</p>}
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-1">
            {blocks.map((block) => (
              <article key={block.id} className={`flex items-center gap-2 rounded-lg border p-2 ${editingId === block.id ? "border-primary" : ""}`}>
                <BlockThumbnail block={block} tiles={tiles} max={64} />
                <div className="min-w-0 flex-1">
                  <div className="truncate">{blockLabel(block)}</div>
                  <div className="text-xs text-muted-foreground">{block.w}×{block.h} tiles</div>
                </div>
                <div className="flex flex-wrap justify-end gap-1">
                  <Button type="button" size="xs" variant="outline" onClick={() => edit(block)}>Edit</Button>
                  <Button type="button" size="xs" variant="ghost" onClick={() => rename(block)}>Rename</Button>
                  <Button type="button" size="xs" variant="ghost" onClick={() => duplicate(block)}>Duplicate</Button>
                  <Button type="button" size="xs" variant="destructive" onClick={() => remove(block)}>Delete</Button>
                </div>
              </article>
            ))}
          </div>
        </section>
      </div>
    </div>
  );
}

export default BlocksTab;
