import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { MapCanvas, drawFrame } from "./canvas";
import type { MapCanvasProps } from "./canvas";
import { emptyMap } from "./state";
import { makeGrid } from "./state";

function props(overrides?: Partial<MapCanvasProps>): MapCanvasProps {
  return {
    map: emptyMap(8, 8),
    tiles: [],
    npcTiles: [],
    layer: "map",
    zoom: 2,
    offsetX: 0,
    offsetY: 0,
    activeBlock: [0],
    blockW: 1,
    blockH: 1,
    selectedNpc: -1,
    selectedScript: -1,
    showChunks: true,
    showChunkHighlight: false,
    showCollision: true,
    showEncounters: false,
    showNpcOverlay: true,
    showScriptOverlay: true,
    onViewChange: () => {},
    onWatchZoom: () => {},
    onPushUndo: () => {},
    onPaintBlock: () => {},
    onPaintCollision: () => {},
    onPaintEncounter: () => {},
    onClickNpc: () => {},
    onClickScript: () => {},
    onRightClick: () => {},
    ...overrides,
  };
}

describe("MapCanvas component", () => {
  it("renders a canvas element", () => {
    const { container } = render(<MapCanvas {...props()} />);
    expect(container.querySelector("canvas")).toBeInTheDocument();
  });

  it("renders with NPCs/scripts/overlays without crashing", () => {
    const map = emptyMap(8, 8);
    map.npcs = [{ id: 0, x: 1, y: 2, sprite_id: 0, script: "" }];
    map.scripts = [{ id: 0, x: 4, y: 4, script: "" }];
    map.collision = makeGrid(8, 8, 0);
    map.collision[2][2] = 1;
    map.collision_types = [{ id: 1, name: "rock", color: "#ff000080" }];
    map.encounters = makeGrid(8, 8, -1);
    map.encounters[0][0] = 0;
    map.encounter_zones = [{ id: 0, name: "zone", rate: 10, creature_ids: [] }];
    const { container } = render(
      <MapCanvas
        {...props({
          map,
          layer: "npc",
          selectedNpc: 0,
          showChunkHighlight: true,
          showEncounters: true,
        })}
      />
    );
    expect(container.querySelector("canvas")).toBeInTheDocument();
  });
});

describe("drawFrame", () => {
  it("draws a frame on a stub context without throwing", () => {
    const canvas = document.createElement("canvas");
    canvas.width = 300;
    canvas.height = 200;
    const ctx = canvas.getContext("2d")!;
    drawFrame(
      ctx,
      300,
      200,
      props({
        layer: "collision",
        showChunkHighlight: true,
        offsetX: 3,
        offsetY: 2,
      }),
      { col: 5, row: 5, visible: true }
    );
    expect(ctx).toBeDefined();
  });
});