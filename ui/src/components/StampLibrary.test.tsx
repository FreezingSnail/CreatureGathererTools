import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StampLibrary } from "./StampLibrary";
import { emptyMap } from "../state";

const tiles = [] as HTMLCanvasElement[];

describe("StampLibrary", () => {
  it("renders, saves selection, deletes, and loads clipboard", () => {
    const map = emptyMap(3, 2);
    map.tiles = [[1, 2, 3], [4, 5, 6]];
    map.stamps = [{ id: 4, name: "old", w: 1, h: 1, tiles: "9" }];
    const onMapChange = vi.fn();
    const onSetClipboard = vi.fn();
    vi.spyOn(window, "prompt").mockReturnValue("new");
    const { rerender } = render(
      <StampLibrary map={map} tiles={tiles} selection={{ x: 1, y: 0, w: 2, h: 2 }} onMapChange={onMapChange} onSetClipboard={onSetClipboard} />
    );
    expect(screen.getByText("old")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Save selection as stamp" }));
    const saved = onMapChange.mock.calls[0][0];
    expect(saved.stamps[saved.stamps.length - 1]).toMatchObject({ id: 5, w: 2, h: 2, tiles: "2,3,5,6", origin: null });
    fireEvent.click(screen.getByRole("button", { name: "Load old" }));
    expect(onSetClipboard).toHaveBeenCalledWith({ w: 1, h: 1, tiles: [9] });
    rerender(<StampLibrary map={saved} tiles={tiles} selection={null} onMapChange={onMapChange} onSetClipboard={onSetClipboard} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete old" }));
    expect(onMapChange.mock.calls[onMapChange.mock.calls.length - 1][0].stamps).toHaveLength(1);
    vi.restoreAllMocks();
  });
});
