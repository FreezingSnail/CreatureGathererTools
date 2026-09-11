import { useEffect, useRef } from "react";
import { resizeGridFromHandle, type ResizeHandle, type ResizeResult, type TileGrid } from "../tileGrid";

const HANDLES: ResizeHandle[] = ["n", "e", "s", "w", "ne", "nw", "se", "sw"];

const HANDLE_LABELS: Record<ResizeHandle, string> = { n: "north edge", e: "east edge", s: "south edge", w: "west edge", ne: "northeast corner", nw: "northwest corner", se: "southeast corner", sw: "southwest corner" };

export interface BlockResizeHandlesProps {
  grid: TileGrid;
  cellPx: number;
  onPreview: (result: ResizeResult | null) => void;
  onCommit: (result: ResizeResult) => void;
}

const supportsX = (handle: ResizeHandle) => handle.includes("e") || handle.includes("w");
const supportsY = (handle: ResizeHandle) => handle.includes("n") || handle.includes("s");

/** Focusable, cell-snapped handles surrounding the block canvas. */
export function BlockResizeHandles({ grid, cellPx, onPreview, onCommit }: BlockResizeHandlesProps) {
  const drag = useRef<{ handle: ResizeHandle; x: number; y: number; grid: TileGrid } | null>(null);
  const preview = useRef<ResizeResult | null>(null);
  const previewCallback = useRef(onPreview);
  const commitCallback = useRef(onCommit);
  previewCallback.current = onPreview;
  commitCallback.current = onCommit;

  useEffect(() => {
    const move = (event: PointerEvent) => {
      const current = drag.current;
      if (!current) return;
      const result = resizeGridFromHandle(
        current.grid,
        current.handle,
        Math.round((event.clientX - current.x) / Math.max(1, cellPx)),
        Math.round((event.clientY - current.y) / Math.max(1, cellPx)),
      );
      preview.current = result;
      previewCallback.current(result);
    };
    const finish = () => {
      if (!drag.current) return;
      const result = preview.current;
      drag.current = null;
      preview.current = null;
      if (result) commitCallback.current(result);
      previewCallback.current(null);
    };
    const cancel = () => {
      if (!drag.current) return;
      drag.current = null;
      preview.current = null;
      previewCallback.current(null);
    };
    const keydown = (event: KeyboardEvent) => {
      if (event.key === "Escape") cancel();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", keydown);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", keydown);
    };
  }, [cellPx]);

  const begin = (handle: ResizeHandle, event: React.PointerEvent<HTMLButtonElement>) => {
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    drag.current = { handle, x: event.clientX, y: event.clientY, grid };
    preview.current = null;
  };

  const nudge = (handle: ResizeHandle, event: React.KeyboardEvent<HTMLButtonElement>) => {
    const deltas: Record<string, [number, number]> = {
      ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0],
    };
    const delta = deltas[event.key];
    if (!delta || (!supportsX(handle) && (event.key === "ArrowLeft" || event.key === "ArrowRight")) || (!supportsY(handle) && (event.key === "ArrowUp" || event.key === "ArrowDown"))) return;
    event.preventDefault();
    const result = resizeGridFromHandle(grid, handle, delta[0], delta[1]);
    onCommit(result);
  };

  return (
    <div className="pointer-events-none absolute -inset-2" aria-describedby="block-resize-instructions">
      {HANDLES.map((handle) => (
        <button
          key={handle}
          type="button"
          data-testid={`resize-handle-${handle}`}
          aria-label={`Resize ${HANDLE_LABELS[handle]}`}
          aria-describedby="block-resize-instructions"
          className={`pointer-events-auto absolute z-10 rounded border border-primary bg-background/90 text-[9px] leading-none focus-visible:ring-2 focus-visible:ring-ring ${
            handle === "n" ? "left-1/2 top-0 h-2 w-8 -translate-x-1/2 cursor-ns-resize" :
            handle === "s" ? "bottom-0 left-1/2 h-2 w-8 -translate-x-1/2 cursor-ns-resize" :
            handle === "e" ? "right-0 top-1/2 h-8 w-2 -translate-y-1/2 cursor-ew-resize" :
            handle === "w" ? "left-0 top-1/2 h-8 w-2 -translate-y-1/2 cursor-ew-resize" :
            handle === "ne" ? "right-0 top-0 h-3 w-3 cursor-nesw-resize" :
            handle === "nw" ? "left-0 top-0 h-3 w-3 cursor-nwse-resize" :
            handle === "se" ? "bottom-0 right-0 h-3 w-3 cursor-nwse-resize" :
            "bottom-0 left-0 h-3 w-3 cursor-nesw-resize"
          }`}
          onPointerDown={(event) => begin(handle, event)}
          onKeyDown={(event) => nudge(handle, event)}
        >
          <span className="sr-only">Arrow keys resize one tile. Escape cancels a drag.</span>
        </button>
      ))}
    </div>
  );
}

export default BlockResizeHandles;
