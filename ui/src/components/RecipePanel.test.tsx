import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { GameMap, WfcProject } from "../types";
import { RecipePanel, parseErrorSlot, recipeMemoryEstimate, recipeSymbolCount, ruleSummary, screenToSlots } from "./RecipePanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mockInvoke = vi.mocked(invoke) as unknown as ReturnType<typeof vi.fn>;

const project = (recipe = true): WfcProject => ({
  version: 1,
  slot_w: 2,
  slot_h: 2,
  pieces: [
    { id: 1, name: "Road", tw: 2, th: 2, tiles: "7x4", weight: 1, limit: null, edge_overrides: [], tags: [] },
    { id: 2, name: "Wall", tw: 4, th: 2, tiles: "8x8", weight: 1, limit: null, edge_overrides: [], tags: [] },
  ],
  socket_labels: [{ id: 3, name: "road", kind: "symmetric" }],
  recipes: recipe ? [{ id: 4, name: "Cave", piece_ids: [1, 2], size_w: 4, size_h: 2, seed: 9, anchors: [], socket_compat: [] }] : [],
});

const map = (wfc: WfcProject): GameMap => ({
  width: 2, height: 2, tiles: [[0, 0], [0, 0]], npcs: [], scripts: [], collision: [[0, 0], [0, 0]], collision_types: [], encounters: [[-1, -1], [-1, -1]], encounter_zones: [], wfc,
});

function renderPanel(initial = project(), saveMap = vi.fn().mockResolvedValue(undefined)) {
  const onProjectChange = vi.fn();
  const onMapChange = vi.fn();
  function Harness() {
    const [current, setCurrent] = useState(initial);
    return <RecipePanel project={current} pieces={current.pieces} tiles={[]} map={map(current)} onProjectChange={(next) => { setCurrent(next); onProjectChange(next); }} onMapChange={onMapChange} saveMap={saveMap} />;
  }
  render(<Harness />);
  return { onProjectChange, onMapChange, saveMap };
}

describe("RecipePanel", () => {
  beforeEach(() => mockInvoke.mockReset());

  it("converts screen presets and estimates recipe-scoped memory", () => {
    expect(screenToSlots(2, 2, 1, 1)).toEqual({ w: 4, h: 2 });
    expect(screenToSlots(4, 2, 2, 2)).toEqual({ w: 4, h: 4 });
    const value = project().recipes[0];
    expect(recipeSymbolCount(value, project().pieces, 2, 2)).toBe(3);
    expect(recipeMemoryEstimate(value, project().pieces, 2, 2)).toBe(192);
  });

  it("creates recipe and edits subset, size, seed through map.wfc", () => {
    const { onProjectChange } = renderPanel(project(false));
    fireEvent.click(screen.getByRole("button", { name: "Create recipe" }));
    expect(onProjectChange.mock.calls[0][0].recipes[0]).toMatchObject({ name: "Recipe 1", piece_ids: [1, 2], seed: 1, size_w: 4, size_h: 2 });
  });

  it("edits subset, seed, and screen size on the selected recipe", () => {
    const { onProjectChange } = renderPanel();
    fireEvent.click(screen.getByLabelText("Recipe piece Wall"));
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].piece_ids).toEqual([1]);
    fireEvent.change(screen.getByLabelText("Recipe seed"), { target: { value: "42" } });
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].seed).toBe(42);
    fireEvent.change(screen.getByLabelText("Hunk size preset"), { target: { value: "two" } });
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0]).toMatchObject({ size_w: 8, size_h: 4 });
  });

  it("places anchors at local slots, rejects out of bounds, and reports orphaned anchors after resize", () => {
    const initial = project();
    initial.recipes[0].size_w = 8;
    initial.recipes[0].size_h = 2;
    const { onProjectChange } = renderPanel(initial);
    fireEvent.click(screen.getByRole("button", { name: "Pick Wall" }));
    fireEvent.click(screen.getByRole("button", { name: "Anchor slot 7,0" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/out of bounds/i);
    fireEvent.click(screen.getByRole("button", { name: "Anchor slot 6,0" }));
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].anchors).toEqual([{ piece_id: 2, sx: 6, sy: 0 }]);
    fireEvent.change(screen.getByLabelText("Hunk size preset"), { target: { value: "one" } });
    expect(screen.getByRole("alert")).toHaveTextContent(/orphaned anchors/i);
  });

  it("places anchors at local slots and rejects overlap", () => {
    const initial = project();
    initial.recipes[0].size_w = 4;
    initial.recipes[0].size_h = 2;
    const { onProjectChange } = renderPanel(initial);
    fireEvent.click(screen.getByRole("button", { name: "Pick Road" }));
    fireEvent.click(screen.getByRole("button", { name: "Anchor slot 1,0" }));
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].anchors).toEqual([{ piece_id: 1, sx: 1, sy: 0 }]);
    fireEvent.click(screen.getByRole("button", { name: "Anchor slot 1,0" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/overlap/i);
  });

  it("generates, rerolls seed, highlights contradiction, and saves RLE stamp origin", async () => {
    const initial = project();
    mockInvoke.mockResolvedValue({ w: 2, h: 2, slot_w: 2, slot_h: 2, tiles: [1, 1, 2, 2], slots: [0], seed: 9, attempts: 3 });
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("generated cave");
    const { onMapChange, saveMap } = renderPanel(initial);
    fireEvent.click(screen.getByRole("button", { name: "Generate" }));
    await waitFor(() => expect(screen.getByText(/attempts 3/i)).toBeInTheDocument());
    expect(mockInvoke).toHaveBeenCalledWith("wfc_generate", expect.objectContaining({ recipeId: 4, seed: 9, size: [4, 2] }));
    fireEvent.click(screen.getByRole("button", { name: "Reroll (seed + 1)" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenLastCalledWith("wfc_generate", expect.objectContaining({ seed: 10 })));
    fireEvent.click(screen.getByRole("button", { name: "Save as stamp" }));
    await waitFor(() => expect(saveMap).toHaveBeenCalled());
    expect(onMapChange.mock.calls[onMapChange.mock.calls.length - 1]?.[0].stamps?.[0]).toMatchObject({ w: 2, h: 2, tiles: "1x2,2x2", origin: { recipe_id: 4, seed: 9, slot_w: 2, slot_h: 2 } });
    prompt.mockRestore();
  });

  it("parses backend slot coordinates without replacing its message", () => {
    expect(parseErrorSlot("WFC generation failed: contradiction at slot (3, 5)")).toEqual({ w: 3, h: 5 });
  });
});

  it("adds, edits, removes rules and renders plain-language summaries", () => {
    const { onProjectChange } = renderPanel();
    fireEvent.click(screen.getByRole("button", { name: "Add count rule" }));
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].rules).toEqual([{ kind: "count", piece_id: 1, min: null, max: null }]);
    fireEvent.change(screen.getByLabelText("Count rule 0 minimum"), { target: { value: "2" } });
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].rules[0]).toMatchObject({ min: 2 });
    expect(screen.getByText("Count Road at least 2")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Remove rule" }));
    expect(onProjectChange.mock.calls[onProjectChange.mock.calls.length - 1]?.[0].recipes[0].rules).toEqual([]);
    expect(ruleSummary({ kind: "exclude", a: 1, b: 2, radius: 1 }, project().pieces)).toBe("Exclude Road and Wall within 1 slot");
  });

  it("bounds hunk history, selects an older hunk, and saves it as a stamp", async () => {
    mockInvoke.mockImplementation(async (_command, args) => {
      const seed = (args as { seed?: number }).seed ?? 0;
      return { w: 2, h: 2, slot_w: 2, slot_h: 2, tiles: [seed, seed, seed, seed], slots: [seed], seed, attempts: 1 };
    });
    const prompt = vi.spyOn(window, "prompt").mockReturnValue("history stamp");
    const { onMapChange } = renderPanel();
    for (let seed = 9; seed < 16; seed += 1) {
      fireEvent.click(seed === 9 ? screen.getByRole("button", { name: "Generate" }) : screen.getByRole("button", { name: "Reroll (seed + 1)" }));
      await waitFor(() => expect(screen.getAllByText(new RegExp(`seed ${seed}`, "i")).length).toBeGreaterThan(0));
    }
    expect(screen.getByText("6/6")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Seed 9/ })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Seed 10/ }));
    fireEvent.click(screen.getByRole("button", { name: "Save as stamp" }));
    await waitFor(() => expect(onMapChange.mock.calls[onMapChange.mock.calls.length - 1]?.[0].stamps?.[0]).toMatchObject({ origin: { seed: 10 } }));
    prompt.mockRestore();
  });
