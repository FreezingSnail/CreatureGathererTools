import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { GameMap, WfcProject } from "../types";
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

  it("blocks slot-size migration and lists offending pieces", () => {
    render(<WfcTab map={map(project({ slot_w: 2, slot_h: 2, pieces: [{ ...project().pieces[0], tw: 2, th: 2 }] }))} />);
    fireEvent.change(screen.getByLabelText("Slot size"), { target: { value: "4x4" } });
    expect(screen.getByRole("alert")).toHaveTextContent(/offending pieces: One/i);
  });

  it("flags an edge with zero legal neighbours", () => {
    render(<WfcTab map={map(project())} />);
    expect(screen.getAllByText(/ZERO/).length).toBeGreaterThan(0);
  });

  it("assigns and clears a label through map.wfc", () => {
    const changes: GameMap[] = [];
    const onMapChange = vi.fn((next: GameMap) => changes.push(next));
    const { rerender } = render(<WfcTab map={map(project())} onMapChange={onMapChange} />);
    fireEvent.change(screen.getByLabelText("Label n0"), { target: { value: "4" } });
    const assigned = changes[changes.length - 1]!;
    expect(assigned.wfc?.pieces[0].edge_overrides).toEqual([{ side: "n", index: 0, label: 4 }]);
    rerender(<WfcTab map={assigned} onMapChange={onMapChange} />);
    fireEvent.change(screen.getByLabelText("Label n0"), { target: { value: "" } });
    expect(changes[changes.length - 1]!.wfc?.pieces[0].edge_overrides).toEqual([]);
  });
});
