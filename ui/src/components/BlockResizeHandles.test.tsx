import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { BlockResizeHandles } from "./BlockResizeHandles";

const grid = { w: 2, h: 2, tiles: [1, 0, 0, 0] };

describe("BlockResizeHandles", () => {
  it("snaps pointer movement to tile cells and commits once", () => {
    const onPreview = vi.fn();
    const onCommit = vi.fn();
    render(<div><BlockResizeHandles grid={grid} cellPx={20} onPreview={onPreview} onCommit={onCommit} /></div>);
    fireEvent.pointerDown(screen.getByTestId("resize-handle-e"), { clientX: 10, clientY: 10, pointerId: 1 });
    fireEvent.pointerMove(window, { clientX: 51, clientY: 10 });
    expect(onPreview.mock.lastCall?.[0].grid.w).toBe(4);
    fireEvent.pointerUp(window);
    expect(onCommit).toHaveBeenCalledTimes(1);
    expect(onCommit.mock.calls[0]![0].grid.w).toBe(4);
    expect(onPreview.mock.lastCall?.[0]).toBeNull();
  });

  it("supports keyboard precision resizing on focused handles", () => {
    const onCommit = vi.fn();
    render(<div><BlockResizeHandles grid={grid} cellPx={20} onPreview={() => undefined} onCommit={onCommit} /></div>);
    fireEvent.keyDown(screen.getByTestId("resize-handle-nw"), { key: "ArrowUp" });
    expect(onCommit).toHaveBeenCalledWith(expect.objectContaining({ grid: expect.objectContaining({ h: 3 }) }));
  });
});
