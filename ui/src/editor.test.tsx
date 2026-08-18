import { describe, expect, it } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MapEditor, type EditorDeps } from "./editor";
import { emptyMap } from "./state";

function deps(): EditorDeps {
  return {
    readBytes: async () => {
      throw new Error("no tauri in tests");
    },
    saveMap: async () => {},
    compileMap: async () => ({
      ok: true,
      errors: [],
      artifacts: ["out/raw_map.bin", "out/scripts.bin"],
    }),
    parseScript: async () => ({ ok: true, errors: [], cmds: null }),
  };
}

const config = {
  creatures_json: "",
  moves_json: "",
  map_json: "",
  tileset_png: "",
  map_tileset_png: "",
  npc_tileset_png: "",
  output_dir: "out",
};

describe("MapEditor", () => {
  it("renders toolbar, canvas, palette and save/compile buttons", () => {
    render(<MapEditor config={config} map={emptyMap(16, 16)} deps={deps()} />);
    expect(screen.getByText(/Layer:/)).toBeInTheDocument();
    expect(screen.getByText("Save")).toBeInTheDocument();
    expect(screen.getByText("Compile")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "undo" })).toBeInTheDocument();
    expect(screen.getByTestId("palette")).toBeInTheDocument();
    expect(screen.getByTestId("zoom-sensitivity")).toBeInTheDocument();
    expect(document.querySelector("canvas")).toBeInTheDocument();
  });

  it("shows palette empty labels when no tilesets load", async () => {
    render(<MapEditor config={config} map={emptyMap(8, 8)} deps={deps()} />);
    expect(
      await screen.findByText("No map tileset loaded (set map_tileset_png)")
    ).toBeInTheDocument();
  });

  it("shows no script pane until an NPC exists", () => {
    const { container } = render(
      <MapEditor config={config} map={emptyMap(16, 16)} deps={deps()} />
    );
    expect(container.querySelector('[data-testid="script-textarea"]')).toBeNull();

    // switch to NPC layer and paint tool, then click the canvas at a tile coordinate
    fireEvent.keyDown(window, { key: "p" });
    fireEvent.change(screen.getByDisplayValue("map"), { target: { value: "npc" } });
    const canvas = container.querySelector("canvas")!;
    fireEvent.mouseDown(canvas, { clientX: 60, clientY: 60, button: 0 });
    fireEvent.mouseUp(canvas);
    expect(screen.getByTestId("script-textarea")).toBeInTheDocument();
  });

  it("saves without empty-script placeholders", async () => {
    const saved: unknown[] = [];
    const d = deps();
    d.saveMap = async (m) => {
      saved.push(m);
    };
    const { container } = render(
      <MapEditor config={config} map={emptyMap(16, 16)} deps={d} />
    );
    fireEvent.keyDown(window, { key: "p" });
    fireEvent.change(screen.getByDisplayValue("map"), { target: { value: "script" } });
    const canvas = container.querySelector("canvas")!;
    fireEvent.mouseDown(canvas, { clientX: 60, clientY: 60, button: 0 });
    fireEvent.mouseUp(canvas);
    fireEvent.click(screen.getByText("Save"));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect((saved[0] as { scripts: unknown[] }).scripts).toHaveLength(0);
  });

  it("left-drag pans instead of placing an NPC", async () => {
    const { container } = render(
      <MapEditor config={config} map={emptyMap(16, 16)} deps={deps()} />
    );
    const canvas = container.querySelector("canvas")!;
    fireEvent.change(screen.getByDisplayValue("map"), { target: { value: "npc" } });
    fireEvent.mouseDown(canvas, { clientX: 60, clientY: 60, button: 0 });
    fireEvent.mouseMove(canvas, { clientX: 100, clientY: 60 });
    fireEvent.mouseMove(canvas, { clientX: 140, clientY: 60 });
    fireEvent.mouseUp(canvas);
    // drag beyond threshold pans; no click action fires (no NPC placed)
    expect(container.querySelector('[data-testid="script-textarea"]')).toBeNull();
  });

  it("compile button surfaces results", async () => {
    render(<MapEditor config={config} map={emptyMap(8, 8)} deps={deps()} />);
    fireEvent.click(screen.getByText("Compile"));
    const result = await screen.findByTestId("compile-result");
    expect(result.textContent).toContain("raw_map.bin");
  });

  it("pan is default and p/g keybinds switch the active tool", () => {
    render(<MapEditor config={config} map={emptyMap(8, 8)} deps={deps()} />);
    const paintBtn = screen.getByRole("button", { name: "paint" });
    const panBtn = screen.getByRole("button", { name: "pan" });
    expect(paintBtn).toHaveAttribute("aria-pressed", "false");
    expect(panBtn).toHaveAttribute("aria-pressed", "true");
    fireEvent.keyDown(window, { key: "p" });
    expect(paintBtn).toHaveAttribute("aria-pressed", "true");
    expect(panBtn).toHaveAttribute("aria-pressed", "false");
    fireEvent.keyDown(window, { key: "g" });
    expect(panBtn).toHaveAttribute("aria-pressed", "true");
  });

  it("pan tool click does not paint or place objects", () => {
    const { container } = render(
      <MapEditor config={config} map={emptyMap(16, 16)} deps={deps()} />
    );
    fireEvent.change(screen.getByDisplayValue("map"), { target: { value: "npc" } });
    const canvas = container.querySelector("canvas")!;
    fireEvent.mouseDown(canvas, { clientX: 60, clientY: 60, button: 0 });
    fireEvent.mouseUp(canvas);
    expect(container.querySelector('[data-testid="script-textarea"]')).toBeNull();
  });

  it("script layer: single click selects, double click opens editor, esc closes", () => {
    const { container } = render(
      <MapEditor config={config} map={emptyMap(16, 16)} deps={deps()} />
    );
    fireEvent.change(screen.getByDisplayValue("map"), { target: { value: "script" } });
    const canvas = container.querySelector("canvas")!;
    // switch to paint so the click places a script object
    fireEvent.keyDown(window, { key: "p" });
    // single click creates + selects only
    fireEvent.mouseDown(canvas, { clientX: 60, clientY: 60, button: 0 });
    fireEvent.mouseUp(canvas);
    expect(container.querySelector('[data-testid="script-textarea"]')).toBeNull();
    // double click opens the inline editor
    fireEvent.dblClick(canvas, { clientX: 60, clientY: 60 });
    expect(screen.getByTestId("script-textarea")).toBeInTheDocument();
    // esc closes it
    fireEvent.keyDown(window, { key: "Escape" });
    expect(container.querySelector('[data-testid="script-textarea"]')).toBeNull();
  });

  it("sidebar resizer drag changes panel width", () => {
    const { container } = render(
      <MapEditor config={config} map={emptyMap(16, 16)} deps={deps()} />
    );
    const resizer = screen.getByTestId("sidebar-resizer");
    const sidebar = container.querySelector(".border-r")!;
    const before = (sidebar as HTMLElement).style.width;
    fireEvent.mouseDown(resizer, { clientX: 100 });
    fireEvent.mouseMove(window, { clientX: 300 });
    fireEvent.mouseUp(window);
    const after = (sidebar as HTMLElement).style.width;
    expect(after).not.toBe(before);
  });
});