import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
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

    // switch to NPC layer, then click the canvas at a tile coordinate
    fireEvent.change(screen.getByDisplayValue("map"), { target: { value: "npc" } });
    const canvas = container.querySelector("canvas")!;
    fireEvent.mouseDown(canvas, { clientX: 60, clientY: 60, button: 0 });
    expect(screen.getByTestId("script-textarea")).toBeInTheDocument();
  });

  it("compile button surfaces results", async () => {
    render(<MapEditor config={config} map={emptyMap(8, 8)} deps={deps()} />);
    fireEvent.click(screen.getByText("Compile"));
    const result = await screen.findByTestId("compile-result");
    expect(result.textContent).toContain("raw_map.bin");
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