import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, afterEach } from "vitest";
import type { Block, GameMap } from "../types";
import { decode } from "../rle";
import { BlockPanel } from "./BlockPanel";

const map = (): GameMap => ({
  width: 3,
  height: 3,
  tiles: [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
  npcs: [],
  scripts: [],
  collision: [],
  collision_types: [],
  encounters: [],
  encounter_zones: [],
});

const block = (): Block => ({ id: 2, name: "hut", w: 2, h: 1, tiles: "4,5", tags: [] });

afterEach(() => vi.restoreAllMocks());

describe("BlockPanel", () => {
  it("arms a block as the paste brush", () => {
    const onUseBlock = vi.fn();
    render(
      <BlockPanel
        blocks={[block()]}
        map={map()}
        tiles={[]}
        selection={null}
        onBlocksChange={vi.fn()}
        onUseBlock={onUseBlock}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Use hut as brush" }));

    expect(onUseBlock).toHaveBeenCalledWith({ w: 2, h: 1, tiles: [4, 5] }, block());
  });

  it("captures the current map selection as a named block", () => {
    const onBlocksChange = vi.fn();
    vi.spyOn(window, "prompt").mockReturnValue("Corner");
    render(
      <BlockPanel
        blocks={[]}
        map={map()}
        tiles={[]}
        selection={{ x: 1, y: 1, w: 2, h: 2 }}
        onBlocksChange={onBlocksChange}
        onUseBlock={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Save selection as block" }));

    const saved = onBlocksChange.mock.calls[0]![0] as Block[];
    expect(saved[0]).toMatchObject({ id: 1, name: "Corner", w: 2, h: 2 });
    expect(decode(saved[0]!.tiles, 4)).toEqual([5, 6, 8, 9]);
  });

  it("disables capture without a selection", () => {
    render(
      <BlockPanel
        blocks={[]}
        map={map()}
        tiles={[]}
        selection={null}
        onBlocksChange={vi.fn()}
        onUseBlock={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Save selection as block" })).toBeDisabled();
    expect(screen.getByText(/No project blocks yet/)).toBeInTheDocument();
  });
});
