import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { CreatureEditor, type CreatureApi } from "./CreatureEditor";
import type { Creature } from "../editorTypes";

const creatures: Creature[] = [
  {
    name: "Floofer",
    id: 1,
    type1: "spirit",
    type2: "none",
    evelevel: 0,
    atk: 10,
    deff: 8,
    spcatk: 6,
    spcdef: 5,
    hp: 30,
    spd: 12,
    moves: ["spirit"],
  },
  {
    name: "Burnling",
    id: 2,
    type1: "fire",
    type2: "none",
    evelevel: 16,
    atk: 12,
    deff: 4,
    spcatk: 14,
    spcdef: 6,
    hp: 25,
    spd: 9,
    moves: ["fire", "spirit"],
  },
];

function makeApi(): CreatureApi & { saveCreatures: ReturnType<typeof vi.fn> } {
  return {
    loadCreatures: vi.fn(async () => creatures),
    saveCreatures: vi.fn(async () => undefined),
  };
}

async function renderEditor() {
  const api = makeApi();
  render(<CreatureEditor api={api} />);
  await screen.findByText("Floofer");
  return api;
}

describe("CreatureEditor", () => {
  it("loads and lists creatures", async () => {
    await renderEditor();
    const root = screen.getByTestId("creature-editor");
    expect(root).toBeInTheDocument();
    expect(root).toHaveClass("h-full", "overflow-auto");
    expect(screen.getByText("Floofer")).toBeInTheDocument();
    expect(screen.getByText("Burnling")).toBeInTheDocument();
  });

  it("populates the form when a creature is selected", async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.click(screen.getByText("Burnling"));
    const name = screen.getByLabelText("Name") as HTMLInputElement;
    expect(name.value).toBe("Burnling");
    expect(screen.getByLabelText("HP")).toHaveValue("25");
    expect(screen.getByLabelText("SPD")).toHaveValue("9");
  });

  it("saves edits through the api", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByText("Floofer"));
    const name = screen.getByLabelText("Name");
    await user.clear(name);
    await user.type(name, "Floofer Prime");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.saveCreatures).toHaveBeenCalledTimes(1));
    const saved = api.saveCreatures.mock.calls[0][0] as Creature[];
    expect(saved[0].name).toBe("Floofer Prime");
    expect(saved[1]).toEqual(creatures[1]);
    expect(await screen.findByRole("status")).toHaveTextContent(/saved 2 creatures/);
  });

  it("rejects malformed stats client-side and does not save", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByText("Floofer"));
    const atk = screen.getByLabelText("ATK");
    await user.clear(atk);
    await user.type(atk, "-5");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("ATK must be a non-negative integer")).toBeInTheDocument();
    expect(api.saveCreatures).not.toHaveBeenCalled();
  });

  it("adds a creature and persists it", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByRole("button", { name: "+ Add" }));
    await waitFor(() => expect(api.saveCreatures).toHaveBeenCalled());
    const saved = api.saveCreatures.mock.calls[0][0] as Creature[];
    expect(saved).toHaveLength(3);
    expect(saved[2].id).toBe(3);
    expect(saved[2].name).toBe("new_creature_3");
  });

  it("shows five duel-type checkboxes reflecting the creature's moves", async () => {
    const user = userEvent.setup();
    await renderEditor();
    await user.click(screen.getByText("Burnling"));
    for (const t of ["spirit", "water", "earth", "fire", "plant"]) {
      expect(screen.getByLabelText(t)).toBeInTheDocument();
    }
    expect(screen.getByLabelText("fire")).toBeChecked();
    expect(screen.getByLabelText("spirit")).toBeChecked();
    expect(screen.getByLabelText("water")).not.toBeChecked();
  });

  it("round-trips toggled attack types through the api", async () => {
    const user = userEvent.setup();
    const api = await renderEditor();
    await user.click(screen.getByText("Floofer"));
    await user.click(screen.getByLabelText("water"));
    expect(screen.getByLabelText("water")).toBeChecked();
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.saveCreatures).toHaveBeenCalledTimes(1));
    const saved = api.saveCreatures.mock.calls[0][0] as Creature[];
    expect(saved[0].moves).toEqual(["spirit", "water"]);
  });
});