// Tile palette: a scrollable grid of tilesets. Shift-click drag-selects a
// row-major rectangle (Go buildPalette parity). Uses toDataURL so jsdom-safe.

import { useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { selectRect } from "./geometry";

export interface PaletteProps {
  tiles: HTMLCanvasElement[];
  perRow: number;
  selected: number[];
  emptyLabel?: string;
  /** Fixed tile width in px. Computed from panel width by the parent so the
   * grid doesn't reflow every pixel during panel resize. */
  tilePx?: number;
  /** Pinch-zoom sensitivity coefficient (higher = snappier). */
  sensitivity?: number;
  /** Distinguishes multiple palettes rendered in one view. */
  testId?: string;
  onSelectBlock: (block: number[], w: number, h: number) => void;
}

let tileIndicesVisible = true;
const tileIndexOverlayListeners = new Set<() => void>();

function getTileIndicesVisible() {
  return tileIndicesVisible;
}

function isTextEntryTarget(target: EventTarget | null) {
  return target instanceof HTMLElement
    && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName));
}

function onGlobalKeyDown(event: KeyboardEvent) {
  if (
    event.defaultPrevented
    || event.key.toLowerCase() !== "n"
    || event.altKey
    || event.ctrlKey
    || event.metaKey
    || event.repeat
    || event.isComposing
    || isTextEntryTarget(event.target)
  ) return;
  event.preventDefault();
  tileIndicesVisible = !tileIndicesVisible;
  tileIndexOverlayListeners.forEach((listener) => listener());
}

function subscribeTileIndexOverlay(listener: () => void) {
  tileIndexOverlayListeners.add(listener);
  if (tileIndexOverlayListeners.size === 1) window.addEventListener("keydown", onGlobalKeyDown);
  return () => {
    tileIndexOverlayListeners.delete(listener);
    if (tileIndexOverlayListeners.size === 0) window.removeEventListener("keydown", onGlobalKeyDown);
  };
}

function TileButton({
  canvas,
  index,
  isSelected,
  showIndex,
  testId,
  onTap,
}: {
  canvas: HTMLCanvasElement;
  index: number;
  isSelected: boolean;
  showIndex: boolean;
  testId: string;
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
    <Button
      type="button"
      variant="ghost"
      title={`tile ${index}: ${canvas.width}x${canvas.height}`}
      onClick={(e) => onTap(e.shiftKey)}
      className={cn(
        "relative aspect-square h-auto w-full shrink-0 overflow-hidden rounded-sm border p-0",
        isSelected
          ? "border-2 border-yellow-400 outline outline-1 outline-black"
          : "border-neutral-700 hover:border-neutral-500"
      )}
      style={{
        backgroundImage: url ? `url(${url})` : undefined,
        backgroundSize: "cover",
        imageRendering: "pixelated",
      }}
      data-testid={testId}
    >
      {showIndex && (
        <span
          className="pointer-events-none absolute left-0 top-0 z-10 bg-blue-500/60 px-0.5 text-[9px] font-semibold leading-tight text-white"
          data-testid="palette-tile-index"
        >
          {index}
        </span>
      )}
    </Button>
  );
}

export function Palette({
  tiles,
  perRow,
  selected,
  emptyLabel = "No tileset loaded",
  tilePx = 48,
  sensitivity = 0.006,
  testId = "palette",
  onSelectBlock,
}: PaletteProps) {
  const anchorRef = useRef(0);
  const gridRef = useRef<HTMLDivElement | null>(null);
  // Zoom lives in a ref and mutates the DOM directly: wheel events fire many
  // times per frame during a trackpad pinch, and React re-rendering the whole
  // grid per tick is what makes pinch lag.
  const zoomRef = useRef(1);
  const selectedSet = useMemo(() => new Set(selected), [selected]);
  const hasTiles = tiles.length > 0;
  const showTileIndices = useSyncExternalStore(
    subscribeTileIndexOverlay,
    getTileIndicesVisible,
    getTileIndicesVisible,
  );

  // Pinch zoom: trackpads emit ctrl+wheel. Native listener so we can
  // preventDefault; plain wheel keeps default scroll behavior. Apply
  // synchronously for snappy feel — the browser coalesces layout at paint, so
  // multiple wheel ticks per frame still cost one layout.
  useEffect(() => {
    if (!hasTiles) return;
    const el = gridRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      // deltaY magnitude drives zoom (deltaMode-normalized): trackpad pinches
      // emit many tiny deltas, so a fixed per-event step over-zooms.
      const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 100 : 1;
      const factor = Math.exp(-e.deltaY * unit * sensitivity);
      zoomRef.current = Math.min(Math.max(zoomRef.current * factor, 0.5), 4);
      el.style.zoom = String(zoomRef.current);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasTiles, sensitivity]);

  if (tiles.length === 0) {
    return (
      <div className="text-muted-foreground" style={{ padding: 8 }} data-testid={testId}>
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
      ref={gridRef}
      style={{
        overflow: "auto",
        maxHeight: "none",
        width: "100%",
        zoom: zoomRef.current,
        display: "grid",
        gridTemplateColumns: `repeat(${cols}, ${tilePx}px)`,
        gap: 2,
        padding: 4,
      }}
      data-testid={testId}
      role="group"
      aria-label={`Tile palette; tile numbers ${showTileIndices ? "shown" : "hidden"}. Press N anywhere outside text fields to toggle.`}
      title={`Tile numbers ${showTileIndices ? "shown" : "hidden"}. Press N anywhere outside text fields to toggle.`}
    >
      {tiles.map((t, i) => (
        <TileButton
          key={i}
          canvas={t}
          index={i}
          isSelected={selectedSet.has(i)}
          showIndex={showTileIndices}
          testId={`${testId}-tile`}
          onTap={(shift) => tap(i, shift)}
        />
      ))}
    </div>
  );
}
