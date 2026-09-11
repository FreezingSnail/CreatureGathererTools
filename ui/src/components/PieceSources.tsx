// WFC sources: direct sprite-sheet capture first; legacy block and fill sources
// remain available under one disclosure. Only WFC piece creation enforces slots.
import { useMemo, useState } from "react";
import type { Block, WfcPiece } from "../types";
import { blockLabel } from "../blocks";
import { alignmentMessage, blocksToPieces, fillPiecesForTiles, isSlotAligned, missingFillTiles, sheetSelectionToPiece } from "../wfcImport";
import { Palette } from "../palette";
import { Button } from "./ui/button";
import { BlockThumbnail } from "./BlockThumbnail";

export interface PieceSourcesProps {
  blocks: Block[];
  pieces: WfcPiece[];
  tiles: HTMLCanvasElement[];
  perRow: number;
  slotW: number;
  slotH: number;
  onAddPieces: (pieces: WfcPiece[]) => void;
}

export function PieceSources({ blocks, pieces, tiles, perRow, slotW, slotH, onAddPieces }: PieceSourcesProps) {
  const [selection, setSelection] = useState<{ tiles: number[]; w: number; h: number }>({ tiles: [], w: 0, h: 0 });
  const [selected, setSelected] = useState<number[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [errors, setErrors] = useState<string[]>([]);
  const sheetTileIds = useMemo(() => tiles.map((_, index) => index), [tiles]);
  const pendingFills = missingFillTiles(pieces, sheetTileIds).length;
  const aligned = useMemo(() => blocks.filter((block) => isSlotAligned(block, slotW, slotH)), [blocks, slotW, slotH]);
  const selectionResult = selection.w > 0 && selection.h > 0
    ? sheetSelectionToPiece(pieces, selection.tiles, selection.w, selection.h, slotW, slotH, "Sheet selection")
    : { error: "Select a rectangle from the active sprite sheet." };
  const selectionError = "error" in selectionResult ? selectionResult.error : null;

  const addSheetPiece = () => {
    if (selectionError) return;
    const entered = window.prompt("WFC piece name", `Sheet ${selection.w}×${selection.h}`);
    if (entered === null) return;
    const name = entered.trim() || `Sheet ${selection.w}×${selection.h}`;
    const result = sheetSelectionToPiece(pieces, selection.tiles, selection.w, selection.h, slotW, slotH, name);
    if ("error" in result) { setMessage(result.error); return; }
    onAddPieces([result.piece]);
    setMessage(`Captured ${result.piece.name} from the sprite sheet.`);
  };

  const toggle = (id: number) => setSelected((current) => current.includes(id) ? current.filter((item) => item !== id) : [...current, id]);
  const importSelected = () => {
    const chosen = blocks.filter((block) => selected.includes(block.id));
    if (!chosen.length) { setMessage("Select at least one block to import."); setErrors([]); return; }
    const { created, errors: skipped } = blocksToPieces(pieces, chosen, slotW, slotH);
    setErrors(skipped);
    if (created.length) { onAddPieces(created); setMessage(`Imported ${created.length} block${created.length === 1 ? "" : "s"} as pieces.`); setSelected([]); }
    else setMessage(null);
  };
  const addFills = () => {
    const created = fillPiecesForTiles(pieces, sheetTileIds, slotW, slotH);
    setErrors([]);
    if (!created.length) { setMessage("Every sheet tile already has a fill piece."); return; }
    onAddPieces(created);
    setMessage(`Added ${created.length} fill piece${created.length === 1 ? "" : "s"}.`);
  };

  return (
    <section aria-label="Piece sources" className="mb-3 rounded-lg border p-3">
      <div className="mb-2">
        <h2 className="font-medium">1. Choose pieces</h2>
        <p className="text-xs text-muted-foreground">Capture an exact rectangle from the active project sprite sheet. WFC pieces must use whole {slotW}×{slotH} slots; ordinary Blocks have no such restriction.</p>
      </div>
      <div className="rounded border bg-muted/20 p-2">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <strong>Sprite-sheet rectangle</strong>
          <span className="text-xs text-muted-foreground">Selection: {selection.w > 0 ? `${selection.w}×${selection.h} tiles` : "none"}</span>
        </div>
        <Palette tiles={tiles} perRow={Math.max(1, perRow)} selected={selection.tiles.filter((tile) => tile >= 0)} tilePx={28} emptyLabel="No active project sprite sheet loaded" testId="wfc-sheet" onSelectBlock={(block, w, h) => { setSelection({ tiles: block, w, h }); setMessage(null); }} />
        <p className={`mt-1 text-xs ${selection.w > 0 && selectionError ? "text-destructive" : "text-muted-foreground"}`} role={selection.w > 0 && selectionError ? "alert" : undefined}>{selectionError ?? `Aligned to ${slotW}×${slotH} WFC slots.`}</p>
        <Button type="button" className="mt-2" onClick={addSheetPiece} disabled={Boolean(selectionError)}>Add selected rectangle as WFC piece</Button>
      </div>

      <details open className="mt-3">
        <summary className="cursor-pointer font-medium">Other sources — existing Blocks and uniform fills</summary>
        <p className="mt-2 text-xs text-muted-foreground">Useful for legacy projects. Imported Blocks still need slot-aligned dimensions; Blocks authored in the Blocks tab remain unrestricted.</p>
        {!blocks.length && <p className="mt-2 rounded border border-dashed p-3 text-xs text-muted-foreground">No project blocks yet. Paint them in the Blocks tab, then import them here.</p>}
        {blocks.length > 0 && <>
          <div className="mt-3 grid max-h-64 gap-2 overflow-auto sm:grid-cols-2 lg:grid-cols-3">
            {blocks.map((block) => {
              const problem = alignmentMessage(block, slotW, slotH);
              return <label key={block.id} className={`flex items-center gap-2 rounded border p-2 ${problem ? "opacity-60" : ""}`}>
                <input type="checkbox" aria-label={`Import ${blockLabel(block)}`} checked={selected.includes(block.id)} disabled={Boolean(problem)} onChange={() => toggle(block.id)} />
                <BlockThumbnail block={block} tiles={tiles} max={48} />
                <span className="min-w-0 flex-1"><span className="block truncate">{blockLabel(block)}</span><span className="block text-xs text-muted-foreground">{block.w}×{block.h} tiles</span>{problem && <span className="block text-xs text-destructive">{problem}</span>}</span>
              </label>;
            })}
          </div>
          <span className="mt-1 block text-xs text-muted-foreground">{aligned.length}/{blocks.length} blocks fit this slot size</span>
        </>}
        <div className="mt-3 flex flex-wrap items-center gap-2">
          <Button type="button" onClick={importSelected} disabled={!selected.length}>Import selected blocks</Button>
          <Button type="button" variant="outline" disabled={!sheetTileIds.length} onClick={addFills}>Add sheet fill pieces{pendingFills ? ` (${pendingFills})` : ""}</Button>
        </div>
      </details>
      {message && <p role="status" className="mt-2 text-xs text-muted-foreground">{message}</p>}
      {errors.length > 0 && <ul role="alert" className="mt-2 list-disc pl-5 text-xs text-destructive">{errors.map((error) => <li key={error}>{error}</li>)}</ul>}
    </section>
  );
}

export default PieceSources;

