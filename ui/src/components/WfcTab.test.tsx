import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { GameMap, WfcProject } from "../types";
import { decode } from "../rle";
import { WfcTab } from "./WfcTab";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke) as unknown as ReturnType<typeof vi.fn>;

const project = (overrides: Partial<WfcProject> = {}): WfcProject => ({
  version: 1, slot_w: 1, slot_h: 1,
  pieces: [{ id: 1, name: "One", tw: 1, th: 1, tiles: "7", weight: 1, limit: null, edge_overrides: [], tags: [] }],
  socket_labels: [{ id: 4, name: "road", kind: "symmetric" }],
  recipes: [],
  ...overrides,
});
const map = (wfc: WfcProject): GameMap => ({
  width: 2, height: 2, tiles: [[0, 0], [0, 0]], npcs: [], scripts: [], collision: [[0, 0], [0, 0]], collision_types: [], encounters: [[-1, -1], [-1, -1]], encounter_zones: [], wfc,
});

beforeEach(() => mockInvoke.mockResolvedValue([]));
afterEach(() => vi.restoreAllMocks());

describe("WfcTab", () => {
  it("renders pieces with thumbnail, dimensions, weight, and limit", () => {
    render(<WfcTab map={map(project())} />);
    expect(screen.getByText("One")).toBeInTheDocument();
    expect(screen.getByLabelText("One thumbnail")).toBeInTheDocument();
    expect(screen.getByLabelText("One weight")).toHaveValue(1);
    expect(screen.getByText("1×1 tiles · 1×1 slots")).toBeInTheDocument();
  });

  it("rejects zero and negative weights inline", () => {
    const onMapChange = vi.fn();
    render(<WfcTab map={map(project())} onMapChange={onMapChange} />);
    const weight = screen.getByLabelText("One weight");
    fireEvent.change(weight, { target: { value: "0" } });
    expect(screen.getByRole("alert")).toHaveTextContent(/greater than zero/i);
    fireEvent.change(weight, { target: { value: "-2" } });
    expect(onMapChange).not.toHaveBeenCalled();
  });

  it("warns with recipe names before deleting a referenced piece", () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<WfcTab map={map(project({ recipes: [{ id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [], socket_compat: [] }] }))} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Cave"));
  });

  it("warns with recipe names before deleting a referenced piece", () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<WfcTab map={map(project({ recipes: [{ id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [], socket_compat: [] }] }))} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("Cave"));
  });

  it("cancels delete-all pieces and disables it for an empty library", () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const onMapChange = vi.fn();
    const { rerender } = render(<WfcTab map={map(project())} onMapChange={onMapChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete all pieces" }));
    expect(confirm).toHaveBeenCalled();
    expect(onMapChange).not.toHaveBeenCalled();
    rerender(<WfcTab map={map(project({ pieces: [] }))} onMapChange={onMapChange} />);
    expect(screen.getByRole("button", { name: "Delete all pieces" })).toBeDisabled();
  });

  it("confirmed delete-all pieces clears every dangling recipe reference", () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const onMapChange = vi.fn();
    const wfc = project({ recipes: [{ id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [{ piece_id: 1, sx: 0, sy: 0 }], socket_compat: [], rules: [{ kind: "count", piece_id: 1, min: 1, max: null }] }] });
    render(<WfcTab map={map(wfc)} onMapChange={onMapChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Delete all pieces" }));
    const recipe = onMapChange.mock.calls[0][0].wfc.recipes[0];
    expect(onMapChange).toHaveBeenCalledTimes(1);
    expect(onMapChange.mock.calls[0][0].wfc.pieces).toEqual([]);
    expect(recipe).toMatchObject({ piece_ids: [], anchors: [], rules: [] });
  });

  it("cancels clear-all anchors without changing the active recipe", () => {
    vi.spyOn(window, "confirm").mockReturnValue(false);
    const onMapChange = vi.fn();
    const wfc = project({ recipes: [{ id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [{ piece_id: 1, sx: 0, sy: 0 }], socket_compat: [] }] });
    const { rerender } = render(<WfcTab map={map(wfc)} onMapChange={onMapChange} />);
    fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear all anchors" }));
    expect(confirm).toHaveBeenCalled();
    expect(onMapChange).not.toHaveBeenCalled();
    rerender(<WfcTab map={map(project({ recipes: [{ id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [], socket_compat: [] }] }))} onMapChange={onMapChange} />);
    expect(screen.getByRole("button", { name: "Clear all anchors" })).toBeDisabled();
  });

  it("confirmed clear-all anchors affects only the active recipe", () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const onMapChange = vi.fn();
    const wfc = project({ recipes: [
      { id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [{ piece_id: 1, sx: 0, sy: 0 }], socket_compat: [] },
      { id: 9, name: "Other", piece_ids: [1], size_w: 2, size_h: 2, seed: 1, anchors: [{ piece_id: 1, sx: 1, sy: 1 }], socket_compat: [] },
    ] });
    render(<WfcTab map={map(wfc)} onMapChange={onMapChange} />);
    fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
    fireEvent.click(screen.getByRole("button", { name: "Clear all anchors" }));
    expect(onMapChange).toHaveBeenCalledTimes(1);
    expect(onMapChange.mock.calls[0][0].wfc.recipes).toMatchObject([{ anchors: [] }, { anchors: [{ piece_id: 1, sx: 1, sy: 1 }] }]);
  });

  it("blocks slot-size migration and lists offending pieces", () => {
    render(<WfcTab map={map(project({ slot_w: 2, slot_h: 2, pieces: [{ ...project().pieces[0], tw: 2, th: 2 }] }))} />);
    fireEvent.change(screen.getByLabelText("WFC slot width"), { target: { value: "4" } });
    expect(screen.getByRole("alert")).toHaveTextContent(/offending pieces: One/i);
  });

  it("flags an edge with zero legal neighbours", () => {
    render(<WfcTab map={map(project())} />);
    fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
    expect(screen.getAllByText(/ZERO/).length).toBeGreaterThan(0);
  });

  it("imports a selected project block as a piece", () => {
    const changes: GameMap[] = [];
    render(
      <WfcTab
        map={map(project({ slot_w: 2, slot_h: 2, pieces: [] }))}
        tiles={Array.from({ length: 4 }, () => document.createElement("canvas"))}
        blocks={[{ id: 3, name: "Hut", w: 2, h: 2, tiles: "4,5,6,7", tags: [] }]}
        onMapChange={(next) => changes.push(next)}
      />,
    );

    fireEvent.click(screen.getByLabelText("Import Hut"));
    fireEvent.click(screen.getByRole("button", { name: "Import selected blocks" }));

    const pieces = changes[changes.length - 1]!.wfc!.pieces;
    expect(pieces).toHaveLength(1);
    expect(pieces[0]).toMatchObject({ name: "Hut", tw: 2, th: 2, tags: ["block"] });
    expect(decode(pieces[0]!.tiles, 4)).toEqual([4, 5, 6, 7]);
  });

  it("refuses to import a block that does not fit the slot size", () => {
    const onMapChange = vi.fn();
    render(
      <WfcTab
        map={map(project({ slot_w: 2, slot_h: 2, pieces: [] }))}
        blocks={[{ id: 1, name: "Odd", w: 3, h: 2, tiles: "1x6", tags: [] }]}
        onMapChange={onMapChange}
      />,
    );

    const checkbox = screen.getByLabelText("Import Odd");
    expect(checkbox).toBeDisabled();
    expect(screen.getByRole("button", { name: "Import selected blocks" })).toBeDisabled();
    expect(onMapChange).not.toHaveBeenCalled();
  });

  it("adds slot-sized fill pieces for the sheet and reports when none remain", () => {
    const changes: GameMap[] = [];
    render(
      <WfcTab
        map={map(project({ slot_w: 2, slot_h: 2, pieces: [] }))}
        tiles={Array.from({ length: 3 }, () => document.createElement("canvas"))}
        onMapChange={(next) => changes.push(next)}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Add sheet fill pieces/ }));

    const pieces = changes[changes.length - 1]!.wfc!.pieces;
    expect(pieces.map((piece) => piece.name)).toEqual(["Fill 0", "Fill 1", "Fill 2"]);
    expect(pieces[1]).toMatchObject({ tw: 2, th: 2, tags: ["fill"] });
    expect(decode(pieces[1]!.tiles, 4)).toEqual([1, 1, 1, 1]);
  });

  it("assigns and clears a label through map.wfc", () => {
    const changes: GameMap[] = [];
    const onMapChange = vi.fn((next: GameMap) => changes.push(next));
    const { rerender } = render(<WfcTab map={map(project())} onMapChange={onMapChange} />);
    fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
    fireEvent.change(screen.getByLabelText("Label n0"), { target: { value: "4" } });
    const assigned = changes[changes.length - 1]!;
    expect(assigned.wfc?.pieces[0].edge_overrides).toEqual([{ side: "n", index: 0, label: 4 }]);
    rerender(<WfcTab map={assigned} onMapChange={onMapChange} />);
    fireEvent.change(screen.getByLabelText("Label n0"), { target: { value: "" } });
    expect(changes[changes.length - 1]!.wfc?.pieces[0].edge_overrides).toEqual([]);
  });
});


  it("captures an aligned sprite-sheet rectangle without backend capture", () => {
    const changes: GameMap[] = [];
    vi.spyOn(window, "prompt").mockReturnValue("Sheet cave");
    render(<WfcTab map={map(project({ slot_w: 2, slot_h: 2, pieces: [] }))} tiles={Array.from({ length: 4 }, () => document.createElement("canvas"))} perRow={2} onMapChange={(next) => changes.push(next)} />);
    const tiles = screen.getAllByTestId("wfc-sheet-tile");
    fireEvent.click(tiles[0]!);
    fireEvent.click(tiles[3]!, { shiftKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Add selected rectangle as WFC piece" }));
    const created = changes[changes.length - 1]!.wfc!.pieces[0]!;
    expect(created).toMatchObject({ id: 1, name: "Sheet cave", tw: 2, th: 2, tags: ["sheet"] });
    expect(decode(created.tiles, 4)).toEqual([0, 1, 2, 3]);
    expect(mockInvoke).not.toHaveBeenCalledWith("capture_piece", expect.anything());
  });


  it("exposes the four-stage workflow and keeps stage panels navigable", () => {
    render(<WfcTab map={map(project())} />);
    expect(screen.getByRole("tab", { name: "Pieces" })).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByRole("tab", { name: "Recipe" }));
    expect(screen.getByRole("tabpanel")).toHaveAttribute("aria-labelledby", "wfc-tab-recipe");
    fireEvent.keyDown(screen.getByRole("tab", { name: "Recipe" }), { key: "ArrowRight" });
    expect(screen.getByRole("tab", { name: "Generate" })).toHaveAttribute("aria-selected", "true");
  });

  it("isolates recipe, generation, and advanced controls by stage", () => {
    const wfc = project({ recipes: [{ id: 8, name: "Cave", piece_ids: [1], size_w: 2, size_h: 2, seed: 7, anchors: [{ piece_id: 1, sx: 0, sy: 0 }], socket_compat: [[4, 4]], rules: [{ kind: "count", piece_id: 1, min: 1, max: null }] }] });
    render(<WfcTab map={map(wfc)} />);

    expect(screen.queryByLabelText("Recipe name")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Generate" })).not.toBeInTheDocument();
    expect(screen.queryByText("Socket inspector")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Recipe" }));
    expect(screen.getByLabelText("Recipe name")).toBeInTheDocument();
    expect(screen.getByLabelText("Hunk size preset")).toBeInTheDocument();
    expect(screen.queryByLabelText("Recipe seed")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Generate" })).not.toBeInTheDocument();
    expect(screen.queryByText("Socket inspector")).not.toBeInTheDocument();
    expect(screen.queryByText("Recipe socket compatibility")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Generate" }));
    expect(screen.getByLabelText("Recipe seed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Generate" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Recipe name")).not.toBeInTheDocument();
    expect(screen.queryByText("Anchors — fixed pieces for generation")).not.toBeInTheDocument();
    expect(screen.queryByText("Recipe socket compatibility")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "Advanced" }));
    expect(screen.getByText("Socket inspector")).toBeInTheDocument();
    expect(screen.getByText("Anchors — fixed pieces for generation")).toBeInTheDocument();
    expect(screen.getByText("Placement rules — optional constraints")).toBeInTheDocument();
    expect(screen.getAllByText("Recipe socket compatibility")).toHaveLength(1);
    expect(screen.queryByLabelText("Recipe name")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Recipe seed")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Generate" })).not.toBeInTheDocument();
  });
