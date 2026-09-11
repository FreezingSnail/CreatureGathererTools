import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, afterEach } from "vitest";
import type { Block } from "../types";
import { decode } from "../rle";
import { BlocksTab } from "./BlocksTab";

const sheet = (count: number) => Array.from({ length: count }, () => document.createElement("canvas"));

const block = (overrides: Partial<Block> = {}): Block => ({
  id: 1,
  name: "hut",
  w: 2,
  h: 1,
  tiles: "4,5",
  tags: [],
  ...overrides,
});

afterEach(() => vi.restoreAllMocks());

describe("BlocksTab", () => {
  it("paints sheet tiles and saves a new block to the library", () => {
    const onBlocksChange = vi.fn();
    render(<BlocksTab blocks={[]} tiles={sheet(4)} perRow={2} onBlocksChange={onBlocksChange} />);

    fireEvent.click(screen.getAllByTestId("block-sheet-tile")[2]!);
    fireEvent.click(screen.getByRole("button", { name: "Create from selection" }));
    fireEvent.change(screen.getByLabelText("Block name"), { target: { value: "Corner" } });
    fireEvent.click(screen.getByRole("button", { name: "Add block" }));

    expect(onBlocksChange).toHaveBeenCalledTimes(1);
    const saved = onBlocksChange.mock.calls[0]![0] as Block[];
    expect(saved).toHaveLength(1);
    expect(saved[0]).toMatchObject({ id: 1, name: "Corner", w: 1, h: 1 });
    expect(decode(saved[0]!.tiles, 1)).toEqual([2]);
  });

  it("paints a multi-tile brush as a single stamp", () => {
    const onBlocksChange = vi.fn();
    render(<BlocksTab blocks={[]} tiles={sheet(4)} perRow={2} onBlocksChange={onBlocksChange} />);

    const tiles = screen.getAllByTestId("block-sheet-tile");
    fireEvent.click(tiles[0]!);
    fireEvent.click(tiles[3]!, { shiftKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Create from selection" }));
    fireEvent.click(screen.getByLabelText("Block cell 0,0 tile 0"));
    fireEvent.click(screen.getByRole("button", { name: "Add block" }));

    const saved = onBlocksChange.mock.calls[0]![0] as Block[];
    expect(decode(saved[0]!.tiles, 4)).toEqual([0, 1, 2, 3]);
  });

  it("resizes the canvas and keeps painted tiles in the overlap", () => {
    const onBlocksChange = vi.fn();
    render(<BlocksTab blocks={[]} tiles={sheet(4)} perRow={2} onBlocksChange={onBlocksChange} />);

    fireEvent.click(screen.getAllByTestId("block-sheet-tile")[1]!);
    fireEvent.click(screen.getByRole("button", { name: "Create from selection" }));
    fireEvent.change(screen.getByLabelText("Block width"), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText("Block height"), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "Add block" }));

    const saved = onBlocksChange.mock.calls[0]![0] as Block[];
    expect(saved[0]).toMatchObject({ w: 3, h: 1 });
    expect(decode(saved[0]!.tiles, 3)).toEqual([1, 0, 0]);
  });

  it("edits an existing block in place", () => {
    const onBlocksChange = vi.fn();
    render(<BlocksTab blocks={[block()]} tiles={sheet(4)} perRow={2} onBlocksChange={onBlocksChange} />);

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.getByText("Editing hut")).toBeInTheDocument();
    fireEvent.click(screen.getAllByTestId("block-sheet-tile")[3]!);
    fireEvent.click(screen.getByLabelText("Block cell 0,0 tile 4"));
    fireEvent.click(screen.getByRole("button", { name: "Save block" }));

    const saved = onBlocksChange.mock.calls[0]![0] as Block[];
    expect(saved).toHaveLength(1);
    expect(saved[0]).toMatchObject({ id: 1, name: "hut", w: 2, h: 1 });
    expect(decode(saved[0]!.tiles, 2)).toEqual([3, 5]);
  });

  it("keeps a populated canvas unchanged when selecting a single palette tile", () => {
    render(<BlocksTab blocks={[block({ w: 2, h: 2, tiles: "1,2,3,4" })]} tiles={sheet(4)} perRow={2} onBlocksChange={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.click(screen.getAllByTestId("block-sheet-tile")[3]!);

    expect(screen.getAllByTestId("block-cell")).toHaveLength(4);
    expect(screen.getByLabelText("Block cell 0,0 tile 1")).toBeInTheDocument();
    expect(screen.getByLabelText("Block cell 1,0 tile 2")).toBeInTheDocument();
    expect(screen.getByLabelText("Block cell 0,1 tile 3")).toBeInTheDocument();
    expect(screen.getByLabelText("Block cell 1,1 tile 4")).toBeInTheDocument();
  });

  it("keeps a populated canvas unchanged when selecting a rectangular palette brush", () => {
    render(<BlocksTab blocks={[block({ w: 2, h: 2, tiles: "1,2,3,4" })]} tiles={sheet(4)} perRow={2} onBlocksChange={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    const tiles = screen.getAllByTestId("block-sheet-tile");
    fireEvent.click(tiles[0]!);
    fireEvent.click(tiles[3]!, { shiftKey: true });

    expect(screen.getAllByTestId("block-cell")).toHaveLength(4);
    expect(screen.getByLabelText("Block cell 0,0 tile 1")).toBeInTheDocument();
    expect(screen.getByLabelText("Block cell 1,0 tile 2")).toBeInTheDocument();
    expect(screen.getByLabelText("Block cell 0,1 tile 3")).toBeInTheDocument();
    expect(screen.getByLabelText("Block cell 1,1 tile 4")).toBeInTheDocument();
  });

  it("duplicates and deletes library blocks", () => {
    const onBlocksChange = vi.fn();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<BlocksTab blocks={[block()]} tiles={sheet(4)} perRow={2} onBlocksChange={onBlocksChange} />);

    fireEvent.click(screen.getByRole("button", { name: "Duplicate" }));
    expect((onBlocksChange.mock.calls[0]![0] as Block[]).map((item) => item.name)).toEqual(["hut", "hut copy"]);

    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onBlocksChange.mock.calls[1]![0]).toEqual([]);
  });

  it("surfaces project save failures", () => {
    render(<BlocksTab blocks={[]} tiles={sheet(1)} perRow={1} onBlocksChange={vi.fn()} saveState="error" saveError="disk full" />);
    expect(screen.getByRole("status")).toHaveTextContent("Save failed: disk full");
  });
});

  it("accepts arbitrary numeric dimensions beyond 32", () => {
    const onBlocksChange = vi.fn();
    render(<BlocksTab blocks={[]} tiles={sheet(1)} perRow={1} onBlocksChange={onBlocksChange} />);
    fireEvent.change(screen.getByLabelText("Block width"), { target: { value: "40" } });
    fireEvent.change(screen.getByLabelText("Block height"), { target: { value: "37" } });
    fireEvent.click(screen.getByRole("button", { name: "Add block" }));
    expect(onBlocksChange.mock.calls[0]![0][0]).toMatchObject({ w: 40, h: 37 });
  });

  it("confirms before shrinking non-empty content", () => {
    const onBlocksChange = vi.fn();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<BlocksTab blocks={[block({ w: 2, h: 2, tiles: "0,7,0,0" })]} tiles={sheet(1)} perRow={1} onBlocksChange={onBlocksChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    fireEvent.change(screen.getByLabelText("Block width"), { target: { value: "1" } });
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("non-empty"));
    fireEvent.click(screen.getByRole("button", { name: "Save block" }));
    expect(onBlocksChange.mock.calls[0]![0][0]).toMatchObject({ w: 2, h: 2 });
  });
