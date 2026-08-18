import { describe, expect, it } from "vitest";
import { fireEvent, render } from "@testing-library/react";
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
    tool: "paint",
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
    onOpenScript: () => {},
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

describe("MapCanvas interactions", () => {
  it("pan tool: left-drag pans and never paints", () => {
    const view: number[][] = [];
    const paints: string[] = [];
    const { container } = render(
      <MapCanvas
        {...props({
          tool: "pan",
          onViewChange: (ox, oy) => view.push([ox, oy]),
          onPaintBlock: () => paints.push("block"),
          onPaintCollision: () => paints.push("collision"),
          onPaintEncounter: () => paints.push("encounter"),
          onPushUndo: () => paints.push("undo"),
          onClickNpc: () => paints.push("npc"),
          onClickScript: () => paints.push("script"),
        })}
      />
    );
    const el = container.firstElementChild!;
    // rect is 0 in jsdom: coords are canvas-local. zoom=2 so tile=32px.
    fireEvent.mouseDown(el, { clientX: 40, clientY: 40, button: 0 });
    fireEvent.mouseMove(el, { clientX: 120, clientY: 80 });
    fireEvent.mouseUp(el);
    expect(view.length).toBe(1);
    expect(view[0][0]).toBeLessThan(0); // panned left/up
    expect(view[0][1]).toBeLessThan(0);
    expect(paints).toEqual([]);
  });

  it("paint tool: sub-threshold drag paints on mouseup", () => {
    const paints: string[] = [];
    const view: number[][] = [];
    const { container } = render(
      <MapCanvas
        {...props({
          tool: "paint",
          onPaintBlock: () => paints.push("block"),
          onPushUndo: () => paints.push("undo"),
          onViewChange: (ox, oy) => view.push([ox, oy]),
        })}
      />
    );
    const el = container.firstElementChild!;
    fireEvent.mouseDown(el, { clientX: 40, clientY: 40, button: 0 });
    fireEvent.mouseMove(el, { clientX: 41, clientY: 41 }); // 1px < threshold
    fireEvent.mouseUp(el);
    expect(paints).toEqual(["undo", "block"]);
    expect(view).toEqual([]);
  });

  it("script layer: double-click calls onOpenScript on an existing script", () => {
    const map = emptyMap(8, 8);
    map.scripts = [{ id: 0, x: 1, y: 1, script: "" }];
    const opened: number[] = [];
    const { container } = render(
      <MapCanvas
        {...props({
          map,
          layer: "script",
          onOpenScript: (id) => opened.push(id),
        })}
      />
    );
    const el = container.firstElementChild!;
    // zoom=2, tile=32px; (60,60) maps to col=1,row=1 (margin 28x20)
    fireEvent.dblClick(el, { clientX: 60, clientY: 60 });
    expect(opened).toEqual([0]);
  });

  it("double-click on map layer does not open scripts", () => {
    const map = emptyMap(8, 8);
    map.scripts = [{ id: 0, x: 1, y: 1, script: "" }];
    const opened: number[] = [];
    const { container } = render(
      <MapCanvas
        {...props({
          map,
          layer: "map",
          onOpenScript: (id) => opened.push(id),
        })}
      />
    );
    fireEvent.dblClick(container.firstElementChild!, { clientX: 60, clientY: 60 });
    expect(opened).toEqual([]);
  });

  it("plain wheel pans; ctrl+wheel zooms gently", () => {
    const view: number[][] = [];
    const zooms: number[] = [];
    const { container } = render(
      <MapCanvas
        {...props({
          onViewChange: (ox, oy) => view.push([ox, oy]),
          onWatchZoom: (z) => zooms.push(z),
        })}
      />
    );
    const el = container.firstElementChild!;
    // zoom=2 => tile=32px; 100px scroll = 3 tiles down, leftovers accumulate
    fireEvent.wheel(el, { deltaY: 100 });
    fireEvent.wheel(el, { deltaY: 1 }); // < tile: no pan yet (accumulated)
    expect(view).toEqual([[0, 3]]);
    // two-finger horizontal scroll pans X
    const h: number[][] = [];
    const { container: c2 } = render(
      <MapCanvas {...props({ onViewChange: (ox, oy) => h.push([ox, oy]) })} />
    );
    fireEvent.wheel(c2.firstElementChild!, { deltaX: 64 });
    expect(h).toEqual([[2, 0]]);
    // ctrl+wheel = pinch: 100px delta per 1.2x notch, leftovers accumulate
    fireEvent.wheel(el, { ctrlKey: true, deltaY: 100 });
    expect(zooms).toEqual([2 / 1.2]);
    // simulate parent applying the zoom: new render with updated zoom prop
    // still uses the same accumulating zoomAccRef gesture state
    const up: number[] = [];
    const { container: c3 } = render(
      <MapCanvas {...props({ zoom: 2 / 1.2, onWatchZoom: (z) => up.push(z) })} />
    );
    fireEvent.wheel(c3.firstElementChild!, { ctrlKey: true, deltaY: 1 });
    fireEvent.wheel(c3.firstElementChild!, { ctrlKey: true, deltaY: 99 });
    expect(up).toEqual([(2 / 1.2) / 1.2]);
    // direction: pinch outward (negative delta) zooms in
    const inWard: number[] = [];
    const { container: c4 } = render(
      <MapCanvas {...props({ onWatchZoom: (z) => inWard.push(z) })} />
    );
    fireEvent.wheel(c4.firstElementChild!, { ctrlKey: true, deltaY: -100 });
    expect(inWard[0]).toBe(2 * 1.2);
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