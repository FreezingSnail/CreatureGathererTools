import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import App from "./App";
import { emptyMap } from "./state";
import type { Config } from "./types";
import type { Creature, Move } from "./editorTypes";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const mockInvoke = vi.mocked(invoke) as unknown as ReturnType<typeof vi.fn>;
const mockOpen = vi.mocked(open);

const config: Config = {
  creatures_json: "creatures.json",
  moves_json: "moves.json",
  map_json: "map.json",
  tileset_png: "tileset.png",
  map_tileset_png: "map_tileset.png",
  npc_tileset_png: "npc_tileset.png",
  output_dir: "out",
};

const map = emptyMap(8, 8);

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
    move1: 0,
    move2: 0,
    move3: 0,
    move4: 0,
    moveList: [],
  },
];

const moves: Move[] = [
  { id: 1, name: "Tackle", type: "spirit", power: 40, physical: 1, accuracy: 100, effect: "NONE" },
];

function mockBoot() {
  mockInvoke.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "load_config":
      case "load_project_config":
        return Promise.resolve(config);
      case "load_map":
        return Promise.resolve(map);
      case "load_creatures":
        return Promise.resolve(creatures);
      case "load_moves":
        return Promise.resolve(moves);
      case "save_map":
        return Promise.resolve(null);
      case "compile_all":
        return Promise.resolve({
          ok: true,
          errors: [],
          artifacts: ["out/raw_map.bin", "out/creatures.bin", "out/moves.bin"],
        });
      case "read_file_bytes":
        return Promise.reject(new Error("no tauri in tests"));
      case "layout_report":
        return Promise.resolve({
          data_page: 0, save_page: 0, data_len: 0, data_len_flash: 0,
          save_len: 0, save_len_flash: 0, page_size: 256, flash_pages: 0, fields: [],
        });
      default:
        return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    }
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockOpen.mockReset();
});

describe("App shell", () => {
  it("shows setup dialog when config is missing and opens editors after saving", async () => {
    const consoleError = vi.spyOn(console, "error");
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_config") return Promise.reject(new Error("missing creature-tools.json"));
      if (cmd === "save_config") return Promise.resolve(config);
      if (cmd === "load_map") return Promise.resolve(map);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const user = userEvent.setup();
    render(<App />);
    expect(await screen.findByTestId("setup-dialog")).toBeInTheDocument();

    await user.type(screen.getByLabelText("Project folder"), "/projects/one");
    await user.type(screen.getByLabelText("Creatures JSON"), "creatures.json");
    await user.type(screen.getByLabelText("Moves JSON"), "moves.json");
    await user.type(screen.getByLabelText("Map JSON"), "map.json");
    await user.type(screen.getByLabelText("Map Tileset PNG"), "map_tileset.png");
    await user.type(screen.getByLabelText("NPC Tileset PNG"), "npc_tileset.png");
    await user.type(screen.getByLabelText("Output Dir"), "out");
    await user.click(screen.getByRole("button", { name: "Create project" }));

    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith(
        "save_config",
        expect.objectContaining({
          projectDir: "/projects/one",
          config: expect.objectContaining({
            creatures_json: "creatures.json",
            moves_json: "moves.json",
            map_json: "map.json",
            map_tileset_png: "map_tileset.png",
            npc_tileset_png: "npc_tileset.png",
            output_dir: "out",
            tileset_png: "",
          }),
        })
      )
    );
    expect(await screen.findByRole("tab", { name: "Map" })).toBeInTheDocument();
    expect(screen.getByTestId("tab-panel-map")).toBeInTheDocument();
    expect(consoleError.mock.calls.flat().join(" ")).not.toContain("Maximum update depth exceeded");
    consoleError.mockRestore();
  });

  it("opens an existing project from the startup setup screen", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_config") return Promise.reject(new Error("no project is open"));
      if (cmd === "load_project_config") return Promise.resolve(config);
      if (cmd === "load_map") return Promise.resolve(map);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    mockOpen.mockResolvedValue("/Users/connorfranc/code/CreatureGathererFX/cgfx-project.json");
    const user = userEvent.setup();
    render(<App />);
    await screen.findByTestId("setup-dialog");

    await user.click(screen.getByRole("button", { name: "Open existing project" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("load_project_config", {
        projectPath: "/Users/connorfranc/code/CreatureGathererFX/cgfx-project.json",
      })
    );
    expect(await screen.findByRole("tab", { name: "Map" })).toBeInTheDocument();
  });

  it("shows a save failure error and keeps the setup dialog open", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_config") return Promise.reject(new Error("missing creature-tools.json"));
      if (cmd === "save_config") return Promise.reject(new Error("permission denied"));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<App />);
    await screen.findByTestId("setup-dialog");
    fireEvent.change(screen.getByLabelText("Project folder"), { target: { value: "/projects/one" } });
    fireEvent.click(screen.getByRole("button", { name: "Create project" }));
    expect(await screen.findByTestId("setup-error")).toHaveTextContent(/save failed/i);
    expect(screen.getByTestId("setup-dialog")).toBeInTheDocument();
  });

  it("renders editors directly when a config exists", async () => {
    mockBoot();
    render(<App />);
    expect(await screen.findByRole("tab", { name: "Map" })).toBeInTheDocument();
    expect(await screen.findByTestId("palette")).toBeInTheDocument();
    expect(screen.getByTestId("tab-panel-creature")).toBeInTheDocument();
    expect(screen.getByTestId("tab-panel-move")).toBeInTheDocument();
    expect(screen.getByTestId("tab-panel-layout")).toBeInTheDocument();
    expect(screen.queryByTestId("setup-dialog")).not.toBeInTheDocument();
  });

  it("hides inactive TabsContent panels while keeping them mounted", async () => {
    mockBoot();
    render(<App />);
    await screen.findByTestId("palette");

    const mapPanel = screen.getByTestId("tab-panel-map");
    const creaturePanel = screen.getByTestId("tab-panel-creature");
    const movePanel = screen.getByTestId("tab-panel-move");
    const layoutPanel = screen.getByTestId("tab-panel-layout");
    const wfcPanel = screen.getByTestId("tab-panel-wfc");

    expect(mapPanel.getAttribute("role")).toBe("tabpanel");
    expect(creaturePanel.getAttribute("role")).toBe("tabpanel");
    expect(movePanel.getAttribute("role")).toBe("tabpanel");
    expect(layoutPanel.getAttribute("role")).toBe("tabpanel");
    expect(wfcPanel.getAttribute("role")).toBe("tabpanel");

    expect(mapPanel).toHaveAttribute("data-state", "active");
    expect(creaturePanel).toHaveAttribute("data-state", "inactive");
    expect(movePanel).toHaveAttribute("data-state", "inactive");
    expect(layoutPanel).toHaveAttribute("data-state", "inactive");
    expect(wfcPanel).toHaveAttribute("data-state", "inactive");

    for (const panel of [mapPanel, creaturePanel, movePanel, layoutPanel, wfcPanel]) {
      expect(panel.className).toContain("data-[state=inactive]:hidden");
    }

    await userEvent.setup().click(screen.getByRole("tab", { name: "WFC" }));
    expect(wfcPanel).toHaveAttribute("data-state", "active");
  });

  it("opens the mounted Layout tab", async () => {
    mockBoot();
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("tab", { name: "Layout" }));
    expect(screen.getByTestId("tab-panel-layout")).toHaveAttribute("data-state", "active");
    expect(await screen.findByTestId("layout-empty")).toBeInTheDocument();
  });

  it("preserves creature editor state across tab switches", async () => {
    mockBoot();
    const user = userEvent.setup();
    render(<App />);
    await screen.findByRole("tab", { name: "Map" });

    await user.click(screen.getByRole("tab", { name: "Creature" }));
    const creaturePanel = screen.getByTestId("tab-panel-creature");
    await within(creaturePanel).findByText("Floofer");
    await user.click(within(creaturePanel).getByText("Floofer"));
    const name = within(creaturePanel).getByLabelText("Name") as HTMLInputElement;
    await user.clear(name);
    await user.type(name, "Floofer Prime");

    await user.click(screen.getByRole("tab", { name: "Map" }));
    expect(screen.getByTestId("tab-panel-map")).toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "Creature" }));
    const reloaded = screen.getByTestId("tab-panel-creature");
    expect(within(reloaded).getByLabelText("Name")).toHaveValue("Floofer Prime");
    // draft persists: the list button still shows the unsaved original name
    expect(within(reloaded).getByText("Floofer")).toBeInTheDocument();
  });

  it("header Save map persists the map via save_map", async () => {
    mockBoot();
    render(<App />);
    await screen.findByTestId("palette");
    fireEvent.click(screen.getByRole("button", { name: "Save map" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("save_map", expect.objectContaining({ map: expect.anything() }))
    );
  });

  it("opens an existing project manifest selected from the native dialog", async () => {
    mockBoot();
    mockOpen.mockResolvedValue("/Users/connorfranc/code/CreatureGathererFX/cgfx-project.json");
    const user = userEvent.setup();
    render(<App />);
    await screen.findByTestId("palette");

    await user.click(screen.getByRole("button", { name: "Open project" }));
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("load_project_config", {
        projectPath: "/Users/connorfranc/code/CreatureGathererFX/cgfx-project.json",
      })
    );
    expect(mockOpen).toHaveBeenCalledWith(expect.objectContaining({
      title: "Open project manifest",
      filters: [{ name: "Project manifest", extensions: ["json"] }],
    }));
  });

  it("header Compile all runs compile_all and surfaces artifacts", async () => {
    mockBoot();
    render(<App />);
    await screen.findByTestId("palette");
    fireEvent.click(screen.getByRole("button", { name: "Compile all" }));
    const result = await screen.findByTestId("app-compile-result");
    expect(result.textContent).toContain("raw_map.bin");
    expect(result.textContent).toContain("moves.bin");
  });

  it("header Compile all surfaces compile errors", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_config") return Promise.resolve(config);
      if (cmd === "load_map") return Promise.resolve(map);
      if (cmd === "load_creatures") return Promise.resolve(creatures);
      if (cmd === "load_moves") return Promise.resolve(moves);
      if (cmd === "compile_all")
        return Promise.resolve({
          ok: false,
          errors: [
            { line: 3, msg: "unknown command" },
            { line: 8, msg: "bad flag" },
          ],
          artifacts: [],
        });
      if (cmd === "read_file_bytes") return Promise.reject(new Error("no tauri in tests"));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<App />);
    await screen.findByTestId("palette");
    fireEvent.click(screen.getByRole("button", { name: "Compile all" }));
    const result = await screen.findByTestId("app-compile-result");
    expect(result.textContent).toContain("line 3: unknown command");
    expect(result.textContent).toContain("line 8: bad flag");
  });

  it("reload config re-runs load_config and returns to setup when config is gone", async () => {
    mockBoot();
    const user = userEvent.setup();
    render(<App />);
    await screen.findByTestId("palette");
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "load_config") return Promise.reject(new Error("config removed"));
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    await user.click(screen.getByRole("button", { name: "Reload config" }));
    expect(await screen.findByTestId("setup-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("setup-error")).toHaveTextContent(/config removed/i);
  });
});