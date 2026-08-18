import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MoveEditor, type MoveApi } from "./MoveEditor";
import type { Move } from "../editorTypes";

const moves: Move[] = [
  { id: 1, name: "Tackle", type: "spirit", power: 40, physical: 1, accuracy: 100, effect: "NONE" },
  { id: 2, name: "Ember", type: "fire", power: 60, physical: 0, accuracy: 90, effect: "KINDLD" },
];

function makeApi(): MoveApi & { saveMoves: ReturnType<typeof vi.fn> } {
  return {
    loadMoves: vi.fn(async () => moves),
    saveMoves: vi.fn(async () => undefined),
  };
}

async function renderEditor() {
  const api = makeApi();
  render(<MoveEditor api={api} />);
  await screen.findByText("Tackle");
  return api;
}

describe("MoveEditor", () => {
  it("loads and lists moves", async () => {
    await renderEditor();
    const root = screen.getByTestId("move-editor");
    expect(root).toBeInTheDocument();
    expect(root).toHaveClass("h-full", "overflow-auto");
    expect(screen.getByText("Tackle")).toBeInTheDocument();
    expect(screen.getByText("Ember")).toBeInTheDocument();
  });

  it("populates the form when a move is selected", async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.click(screen.getByText("Ember"));
    expect(screen.getByLabelText("Name")).toHaveValue("Ember");
    expect(screen.getByLabelText("Power")).toHaveValue("60");
    expect(screen.getByLabelText("Accuracy")).toHaveValue("90");
    expect(screen.getByLabelText("Effect")).toHaveValue("KINDLD");
  });

  it("saves edits through the api", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByText("Tackle"));
    const power = screen.getByLabelText("Power");
    await user.clear(power);
    await user.type(power, "55");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.saveMoves).toHaveBeenCalledTimes(1));
    const saved = api.saveMoves.mock.calls[0][0] as Move[];
    expect(saved[0]).toMatchObject({ id: 1, name: "Tackle", power: 55 });
    expect(saved[1]).toEqual(moves[1]);
    expect(await screen.findByRole("status")).toHaveTextContent(/saved 2 moves/);
  });

  it("rejects malformed accuracy and does not save", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByText("Tackle"));
    const acc = screen.getByLabelText("Accuracy");
    await user.clear(acc);
    await user.type(acc, "12.5");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(
      await screen.findByText("Accuracy must be a non-negative integer")
    ).toBeInTheDocument();
    expect(api.saveMoves).not.toHaveBeenCalled();
  });

  it("adds a move and persists it", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByRole("button", { name: "+ Add" }));
    await waitFor(() => expect(api.saveMoves).toHaveBeenCalled());
    const saved = api.saveMoves.mock.calls[0][0] as Move[];
    expect(saved).toHaveLength(3);
    expect(saved[2]).toMatchObject({ id: 3, name: "new_move_3", accuracy: 100 });
  });
});