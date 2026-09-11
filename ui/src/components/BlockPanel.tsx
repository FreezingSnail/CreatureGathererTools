// Map-editor block panel: arm a project block as the paste brush, or capture
// the current map selection as a new block. Block authoring itself lives in
// the Blocks tab; this panel only consumes and captures.

import type { Block, TileRect } from "../types";
import type { TileClipboard } from "../state";
import { blockClipboard, blockFromGrid, blockLabel, gridFromMap, nextBlockId, upsertBlock } from "../blocks";
import type { GameMap } from "../types";
import { Button } from "./ui/button";
import { BlockThumbnail } from "./BlockThumbnail";

export interface BlockPanelProps {
  blocks: Block[];
  map: GameMap;
  tiles: HTMLCanvasElement[];
  selection: TileRect | null;
  /** Block currently armed as the paste brush, if any. */
  activeBlockId?: number | null;
  onBlocksChange: (blocks: Block[]) => void;
  onUseBlock: (clipboard: TileClipboard, block: Block) => void;
}

export function BlockPanel({
  blocks,
  map,
  tiles,
  selection,
  activeBlockId = null,
  onBlocksChange,
  onUseBlock,
}: BlockPanelProps) {
  const captureSelection = () => {
    if (!selection) return;
    const id = nextBlockId(blocks);
    const name = window.prompt("Block name", `Block ${id}`)?.trim();
    if (!name) return;
    onBlocksChange(upsertBlock(blocks, blockFromGrid(id, name, gridFromMap(map, selection))));
  };

  return (
    <section data-testid="block-panel" className="border-t p-2">
      <div className="flex items-center justify-between gap-2">
        <strong className="text-xs">Blocks</strong>
        <Button type="button" size="sm" variant="outline" disabled={!selection} onClick={captureSelection}>
          Save selection as block
        </Button>
      </div>
      {!blocks.length && (
        <p className="mt-1.5 text-[11px] text-muted-foreground">No project blocks yet. Paint them in the Blocks tab.</p>
      )}
      <div className="mt-1.5 flex flex-col gap-1.5">
        {blocks.map((block) => (
          <article key={block.id} className={`flex items-center gap-1.5 rounded border p-1 ${activeBlockId === block.id ? "border-primary" : "border-transparent"}`}>
            <BlockThumbnail block={block} tiles={tiles} max={48} />
            <div className="min-w-0 flex-1">
              <div className="truncate text-xs">{blockLabel(block)}</div>
              <div className="text-[10px] text-muted-foreground">{block.w}×{block.h}</div>
            </div>
            <Button
              type="button"
              size="xs"
              variant="outline"
              aria-label={`Use ${blockLabel(block)} as brush`}
              onClick={() => onUseBlock(blockClipboard(block), block)}
            >
              Brush
            </Button>
          </article>
        ))}
      </div>
    </section>
  );
}

export default BlockPanel;
