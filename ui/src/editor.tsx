// Map editor assembly: toolbar + palette sidebar + canvas + inline script
// editor + save/compile. Drives the Tauri command layer via injected async
// functions so components stay testable outside the webview.

import {
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
  type MouseEvent as ReactMouseEvent,
  type MutableRefObject,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config, GameMap, ParseResult, CompileResult, TileRect, WfcProject, WfcPiece } from "./types";
import { StampLibrary } from "./components/StampLibrary";
import {
  initialState,
  reducer,
  LAYERS,
  type LayerType,
  type EditorTool,
} from "./state";
import {
  Tileset,
  loadMapTileset,
  loadTilesetFromBytes,
  emptyTileset,
} from "./tileset";
import { clampRect } from "./geometry";
import { MapCanvas } from "./canvas";
import { Palette } from "./palette";
import { ScriptPane } from "./scriptPane";
import { Button } from "@/components/ui/button";

/** Call Tiled import with the open map so backend merge preserves WFC data. */
export function importTiledMap(path: string, current: GameMap): Promise<GameMap> {
  return invoke<GameMap>("tiled_import_path", { path, current });
}

export interface EditorDeps {
  readBytes: (path: string) => Promise<Uint8Array>;
  saveMap: (map: GameMap) => Promise<void>;
  compileMap: () => Promise<CompileResult>;
  parseScript: (src: string) => Promise<ParseResult>;
}

export function defaultWfcProject(): WfcProject {
  return { version: 1, slot_w: 2, slot_h: 2, pieces: [], socket_labels: [], recipes: [] };
}

function snappedCaptureRect(rect: TileRect, project: WfcProject): TileRect | null {
  const x = Math.floor(rect.x / project.slot_w) * project.slot_w;
  const y = Math.floor(rect.y / project.slot_h) * project.slot_h;
  const endX = Math.ceil((rect.x + rect.w) / project.slot_w) * project.slot_w;
  const endY = Math.ceil((rect.y + rect.h) / project.slot_h) * project.slot_h;
  return clampRect({ x, y, w: endX - x, h: endY - y }, 256, 256);
}

interface EditorProps {
  config: Config;
  map: GameMap;
  deps: EditorDeps;
  /** Gate global key handlers when the editor is hidden behind another tab. */
  active?: boolean;
  /** Shell can trigger the editor's own save (header Save map button). */
  saveRef?: MutableRefObject<(() => void) | null>;
}

type PaletteTab = "map" | "npc" | "collision" | "encounter";

export function MapEditor({
  config,
  map,
  deps,
  active = true,
  saveRef,
}: EditorProps) {
  const [state, dispatch] = useReducer(reducer, undefined, initialState);
  const [mapTiles, setMapTiles] = useState<Tileset>(emptyTileset);
  const [npcTiles, setNpcTiles] = useState<Tileset>(emptyTileset);
  const [paletteTab, setPaletteTab] = useState<PaletteTab>("map");
  const [sidebarW, setSidebarW] = useState<number | null>(null);
  const [zoomSens, setZoomSens] = useState<number>(() => {
    try {
      const v = Number(localStorage.getItem("cgfx.paletteZoomSensitivity"));
      return Number.isFinite(v) && v > 0 ? v : 0.006;
    } catch {
      return 0.006;
    }
  });
  const sidebarRef = useRef<HTMLDivElement | null>(null);
  const statusRef = useRef("");

  // load the map (once)
  useEffect(() => {
    dispatch({ type: "SET_MAP", map });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // load tilesets from config paths
  useEffect(() => {
    let cancelled = false;
    loadMapTileset(deps.readBytes, config.map_tileset_png, config.tileset_png).then(
      (t) => {
        if (!cancelled) {
          setMapTiles(t);
        }
      }
    );
    if (config.npc_tileset_png) {
      deps
        .readBytes(config.npc_tileset_png)
        .then((bytes) => loadTilesetFromBytes(bytes))
        .then((t) => {
          if (!cancelled) {
            setNpcTiles(t);
          }
        })
        .catch(() => {
          if (!cancelled) {
            setNpcTiles(emptyTileset());
          }
        });
    }
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [config.map_tileset_png, config.npc_tileset_png, config.tileset_png]);

  // advisory script parse (debounced)
  const selectedScriptKey =
    state.selectedNpc >= 0
      ? `npc:${state.selectedNpc}`
      : state.editingScript >= 0
        ? `script:${state.editingScript}`
        : null;
  const selectedScriptText =
    state.selectedNpc >= 0 && state.map.npcs[state.selectedNpc]
      ? state.map.npcs[state.selectedNpc].script
      : state.editingScript >= 0 && state.map.scripts[state.editingScript]
        ? state.map.scripts[state.editingScript].script
        : "";
  useEffect(() => {
    if (!active) {
      return;
    }
    if (!selectedScriptKey) {
      return;
    }
    const handle = window.setTimeout(() => {
      deps.parseScript(selectedScriptText).then((result) => {
        dispatch({ type: "SET_PARSE_RESULT", key: selectedScriptKey, result });
      });
    }, 400);
    return () => window.clearTimeout(handle);
  }, [selectedScriptKey, selectedScriptText, deps, active]);

  // keyboard shortcuts: Ctrl+Z undo, Ctrl+= zoom in, Ctrl+- zoom out, H highlight,
  // P paint tool, G pan tool. Ignored while typing in fields.
  useEffect(() => {
    if (!active) {
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const editing =
        !!target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable);
      if (editing) {
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        dispatch({ type: "UNDO" });
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
        e.preventDefault();
        dispatch({ type: "COPY_SELECTION" });
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") {
        e.preventDefault();
        dispatch({ type: "SET_TOOL", tool: "paste" });
      } else if ((e.ctrlKey || e.metaKey) && e.key === "=") {
        e.preventDefault();
        dispatch({ type: "SET_ZOOM", zoom: state.zoom * 1.5 });
      } else if ((e.ctrlKey || e.metaKey) && e.key === "-") {
        e.preventDefault();
        dispatch({ type: "SET_ZOOM", zoom: state.zoom / 1.5 });
      } else if (e.key.toLowerCase() === "h") {
        dispatch({ type: "TOGGLE_CHUNK_HIGHLIGHT" });
      } else if (e.key.toLowerCase() === "p") {
        dispatch({ type: "SET_TOOL", tool: "paint" });
      } else if (e.key.toLowerCase() === "g") {
        dispatch({ type: "SET_TOOL", tool: "pan" });
      } else if (e.key.toLowerCase() === "s") {
        dispatch({ type: "SET_TOOL", tool: "select" });
      } else if (e.key.toLowerCase() === "v") {
        dispatch({ type: "SET_TOOL", tool: "paste" });
      } else if (e.key === "Escape") {
        dispatch({ type: "OPEN_SCRIPT", id: -1 });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state.zoom, active]);

  const setStatus = useCallback((msg: string) => {
    statusRef.current = msg;
    dispatch({ type: "SET_STATUS", status: msg });
  }, []);

  const save = useCallback(async () => {
    try {
      // scripts without any text are dead placeholders from panel clicks — drop
      const map = {
        ...state.map,
        scripts: state.map.scripts.filter((s) => s.script.trim() !== ""),
      };
      await deps.saveMap(map);
      setStatus("map saved");
    } catch (err) {
      setStatus(`save failed: ${String(err)}`);
    }
  }, [state.map, deps, setStatus]);

  // Expose the editor's save through the shell-provided ref (header Save map).
  useEffect(() => {
    if (!saveRef) {
      return;
    }
    saveRef.current = save;
    return () => {
      saveRef.current = null;
    };
  }, [save, saveRef]);

  const compile = useCallback(async () => {
    setStatus("compiling…");
    try {
      const result = await deps.compileMap();
      dispatch({ type: "SET_COMPILE_RESULT", result });
      setStatus(result.ok ? "compile ok" : `compile failed (${result.errors.length} errors)`);
    } catch (err) {
      setStatus(`compile error: ${String(err)}`);
    }
  }, [deps, setStatus]);

  const capturePiece = useCallback(async () => {
    const rect = state.selection;
    if (!rect) {
      setStatus("select tiles first");
      return;
    }
    const name = window.prompt("Piece name", "captured piece")?.trim();
    if (!name) return;
    const project = state.map.wfc ?? defaultWfcProject();
    if (project.slot_w <= 0 || project.slot_h <= 0) {
      setStatus("capture failed: invalid WFC slot size");
      return;
    }
    try {
      const piece = await invoke<WfcPiece>("capture_piece", {
        map: { ...state.map, wfc: project },
        rect,
        opts: { name, weight: 1, limit: null },
      });
      const nextProject = { ...project, pieces: [...project.pieces, piece] };
      dispatch({ type: "UPDATE_MAP", map: { ...state.map, wfc: nextProject } });
      const snapped = snappedCaptureRect(rect, project);
      if (snapped) dispatch({ type: "SET_SELECTION", rect: snapped });
      setStatus(`captured ${name} (${piece.tw}×${piece.th})`);
    } catch (err) {
      setStatus(`capture failed: ${String(err)}`);
    }
  }, [state.map, state.selection, setStatus]);

  const selectedNpcs = state.selectedNpc >= 0 ? [state.map.npcs[state.selectedNpc]] : [];
  const editingScripts = state.editingScript >= 0 ? [state.map.scripts[state.editingScript]] : [];

  // Tile grid fills the panel width (1fr columns); size the sidebar to fit a
  // full sheet row by default, or keep the user's dragged width.
  const palettePerRow =
    paletteTab === "map" ? mapTiles.perRow : paletteTab === "npc" ? npcTiles.perRow : 0;
  const defaultFit = Math.min(Math.max(230, palettePerRow * 50 + 24), 640);
  const sidebarWidth = Math.min(Math.max(sidebarW ?? defaultFit, 160), 960);
  const widthRef = useRef(sidebarWidth);
  widthRef.current = sidebarWidth;

  // Fixed tile px for the active palette, derived from the current panel
  // width. Recomputed only on re-render (i.e. mouseup), so dragging the panel
  // doesn't reflow every tile each frame.
  const paletteTilePx = (perRow: number) => {
    const cols = Math.max(1, perRow);
    const avail = sidebarWidth - 8 - (cols - 1) * 2;
    return Math.max(20, Math.floor(avail / cols));
  };

  const toggle = (label: string, checked: boolean, onChange: () => void) => (
    <label style={{ fontSize: 12, display: "inline-flex", alignItems: "center", gap: 3 }}>
      <input type="checkbox" checked={checked} onChange={onChange} />
      {label}
    </label>
  );

  const startResize = (e: ReactMouseEvent) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = widthRef.current;
    const onMove = (ev: MouseEvent) => {
      const w = Math.min(Math.max(startW + ev.clientX - startX, 160), 960);
      // Mutate the DOM directly during drag: React re-render per mousemove
      // rebuilds the canvas tree and causes flicker/stall.
      widthRef.current = w;
      if (sidebarRef.current) {
        sidebarRef.current.style.width = `${w}px`;
      }
    };
    const onUp = (ev: MouseEvent) => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      const w = Math.min(Math.max(startW + ev.clientX - startX, 160), 960);
      setSidebarW(w);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  };

  return (
    <div style={{ display: "flex", height: "100%", fontFamily: "system-ui, sans-serif" }}>
      {/* sidebar */}
      <div
        ref={sidebarRef}
        className="border-r border-neutral-800 bg-neutral-900"
        style={{
          width: sidebarWidth,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4, padding: 6 }}>
          {(["map", "npc", "collision", "encounter"] as PaletteTab[]).map((t) => (
            <Button
              key={t}
              type="button"
              size="sm"
              variant={paletteTab === t ? "default" : "ghost"}
              onClick={() => {
                setPaletteTab(t);
                dispatch({ type: "SET_TOOL", tool: "paint" });
                dispatch({ type: "SET_LAYER", layer: t as LayerType });
              }}
            >
              {t}
            </Button>
          ))}
        </div>
        <div style={{ flex: 1, overflow: "auto", padding: 4 }}>
          {paletteTab === "map" && (
            <>
              <Palette
                tiles={mapTiles.tiles}
                perRow={mapTiles.perRow}
                tilePx={paletteTilePx(mapTiles.perRow)}
                sensitivity={zoomSens}
                selected={state.activeBlock}
                onSelectBlock={(block, w, h) => {
                  dispatch({ type: "SET_TOOL", tool: "paint" });
                  dispatch({ type: "SET_ACTIVE_BLOCK", block, w, h });
                }}
                emptyLabel="No map tileset loaded (set map_tileset_png)"
              />
              <StampLibrary
                map={state.map}
                tiles={mapTiles.tiles}
                selection={state.selection}
                onMapChange={(next) => dispatch({ type: "UPDATE_MAP", map: next })}
                onSetClipboard={(clipboard) => dispatch({ type: "SET_CLIPBOARD", clipboard })}
              />
            </>
          )}
          {paletteTab === "npc" && (
            <Palette
              tiles={npcTiles.tiles}
              perRow={npcTiles.perRow}
              tilePx={paletteTilePx(npcTiles.perRow)}
              sensitivity={zoomSens}
              selected={[state.activeNpcSprite]}
              onSelectBlock={(block) => {
                dispatch({ type: "SET_TOOL", tool: "paint" });
                dispatch({ type: "SET_NPC_SPRITE", sprite: block[0] ?? 0 });
              }}
              emptyLabel="No NPC tileset loaded (set npc_tileset_png)"
            />
          )}
          {paletteTab === "collision" && (
            <div style={{ padding: 8, display: "flex", flexDirection: "column", gap: 8 }}>
              <label style={{ fontSize: 12 }}>
                Active collision type
                <select
                  value={state.activeCollisionType}
                  onChange={(e) => {
                      dispatch({ type: "SET_TOOL", tool: "paint" });
                      dispatch({ type: "SET_COLLISION_TYPE", id: Number(e.target.value) });
                    }}
                  style={{ display: "block", width: "100%", marginTop: 4 }}
                >
                  {state.map.collision_types.map((ct) => (
                    <option key={ct.id} value={ct.id}>
                      {ct.name}
                    </option>
                  ))}
                </select>
              </label>
              {state.map.collision_types.length === 0 && (
                <span className="text-muted-foreground" style={{ fontSize: 11 }}>
                  No collision types in map JSON — painting uses type 1.
                </span>
              )}
              <label style={{ fontSize: 12 }}>
                Brush size
                <input
                  type="number"
                  min={1}
                  max={16}
                  value={state.brushSize}
                  onChange={(e) =>
                    dispatch({ type: "SET_BRUSH_SIZE", size: Number(e.target.value) })
                  }
                  style={{ display: "block", width: "100%", marginTop: 4 }}
                />
              </label>
            </div>
          )}
          {paletteTab === "encounter" && (
            <div style={{ padding: 8, display: "flex", flexDirection: "column", gap: 8 }}>
              <label style={{ fontSize: 12 }}>
                Active encounter zone
                <select
                  value={state.activeEncounterZone}
                  onChange={(e) => {
                      dispatch({ type: "SET_TOOL", tool: "paint" });
                      dispatch({ type: "SET_ENCOUNTER_ZONE", id: Number(e.target.value) });
                    }}
                  style={{ display: "block", width: "100%", marginTop: 4 }}
                >
                  {state.map.encounter_zones.map((z) => (
                    <option key={z.id} value={z.id}>
                      {z.name} ({z.rate}%)
                    </option>
                  ))}
                </select>
              </label>
              {state.map.encounter_zones.length === 0 && (
                <span className="text-muted-foreground" style={{ fontSize: 11 }}>
                  No encounter zones in map JSON — painting uses zone 0.
                </span>
              )}
              <label style={{ fontSize: 12 }}>
                Brush size
                <input
                  type="number"
                  min={1}
                  max={16}
                  value={state.brushSize}
                  onChange={(e) =>
                    dispatch({ type: "SET_BRUSH_SIZE", size: Number(e.target.value) })
                  }
                  style={{ display: "block", width: "100%", marginTop: 4 }}
                />
              </label>
            </div>
          )}
        </div>
        <div
          className="border-t border-neutral-800 text-muted-foreground"
          style={{ padding: 6, fontSize: 11 }}
        >
          <label className="mb-1 flex items-center justify-between gap-2">
            <span>Pinch zoom sensitivity</span>
            <span className="tabular-nums">{zoomSens.toFixed(3)}</span>
          </label>
          <input
            type="range"
            min={0.001}
            max={0.02}
            step={0.001}
            value={zoomSens}
            onChange={(e) => {
              const v = Number(e.target.value);
              setZoomSens(v);
              try {
                localStorage.setItem("cgfx.paletteZoomSensitivity", String(v));
              } catch {
                // ignore storage failures (tests / restricted webview)
              }
            }}
            className="w-full"
            data-testid="zoom-sensitivity"
          />
          <div className="mt-1">
            wheel/2-finger: pan · pinch or ctrl+wheel: zoom · p/g: paint/pan ·
            alt/middle-drag: pan · right-click: delete/clear · dbl-click: edit script ·
            esc: close · ctrl+z: undo
          </div>
        </div>
      </div>

      {/* drag handle to resize the palette panel */}
      <div
        className="w-1.5 shrink-0 cursor-col-resize bg-neutral-700 hover:bg-neutral-500"
        onMouseDown={startResize}
        data-testid="sidebar-resizer"
      />

      {/* main column */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
        {/* toolbar */}
        <div
          className="border-b border-neutral-800 bg-neutral-800/80"
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            padding: "6px 10px",
            flexWrap: "wrap",
          }}
        >
          <span style={{ fontSize: 12 }}>Tool:</span>
          {(["paint", "pan", "select", "paste"] as EditorTool[]).map((t) => (
            <Button
              key={t}
              type="button"
              variant={state.tool === t ? "default" : "ghost"}
              size="sm"
              aria-pressed={state.tool === t}
              onClick={() => dispatch({ type: "SET_TOOL", tool: t })}
            >
              {t}
            </Button>
          ))}
          <span style={{ fontSize: 12, marginLeft: 8 }}>Layer:</span>
          <select
            value={state.layer}
            onChange={(e) => dispatch({ type: "SET_LAYER", layer: e.target.value as LayerType })}
            style={{ fontSize: 12 }}
          >
            {LAYERS.map((l) => (
              <option key={l} value={l}>
                {l}
              </option>
            ))}
          </select>
          <span style={{ fontSize: 12, marginLeft: 8 }}>Tile: {state.activeTile}</span>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => dispatch({ type: "SET_BRUSH_SIZE", size: state.brushSize - 1 })}
          >
            -
          </Button>
          <span style={{ fontSize: 12 }}>brush {state.brushSize}</span>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={() => dispatch({ type: "SET_BRUSH_SIZE", size: state.brushSize + 1 })}
          >
            +
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={state.undoStack.length === 0}
            onClick={() => dispatch({ type: "UNDO" })}
          >
            undo
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!state.selection}
            onClick={() => dispatch({ type: "COPY_SELECTION" })}
          >
            Copy
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!state.selection}
            onClick={() => dispatch({ type: "CLEAR_SELECTION" })}
          >
            Clear selection
          </Button>
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={!state.selection}
            onClick={capturePiece}
          >
            Capture as piece
          </Button>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => dispatch({ type: "SET_ZOOM", zoom: state.zoom / 1.5 })}
          >
            -Z
          </Button>
          <span style={{ fontSize: 12 }}>{Math.round(state.zoom * 10) / 10}x</span>
          <Button
            type="button"
            variant="outline"
            size="icon-sm"
            onClick={() => dispatch({ type: "SET_ZOOM", zoom: state.zoom * 1.5 })}
          >
            +Z
          </Button>
          {toggle("chunks", state.showChunks, () => dispatch({ type: "TOGGLE_CHUNKS" }))}
          {toggle("highlight", state.showChunkHighlight, () =>
            dispatch({ type: "TOGGLE_CHUNK_HIGHLIGHT" })
          )}
          {toggle("collision", state.showCollision, () => dispatch({ type: "TOGGLE_COLLISION" }))}
          {toggle("encounters", state.showEncounters, () =>
            dispatch({ type: "TOGGLE_ENCOUNTERS" })
          )}
          {toggle("npcs", state.showNpcOverlay, () => dispatch({ type: "TOGGLE_NPC_OVERLAY" }))}
          {toggle("scripts", state.showScriptOverlay, () =>
            dispatch({ type: "TOGGLE_SCRIPT_OVERLAY" })
          )}
          <span style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
            <Button type="button" variant="default" size="sm" onClick={save}>
              Save
            </Button>
            <Button type="button" variant="outline" size="sm" onClick={compile}>
              Compile
            </Button>
          </span>
        </div>

        {/* canvas */}
        <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
          <MapCanvas
            map={state.map}
            tiles={mapTiles.tiles}
            npcTiles={npcTiles.tiles}
            layer={state.layer}
            tool={state.tool}
            zoom={state.zoom}
            offsetX={state.offsetX}
            offsetY={state.offsetY}
            activeBlock={state.activeBlock}
            blockW={state.blockW}
            blockH={state.blockH}
            selection={state.selection}
            clipboard={state.clipboard}
            selectedNpc={state.selectedNpc}
            selectedScript={state.selectedScript}
            showChunks={state.showChunks}
            showChunkHighlight={state.showChunkHighlight}
            showCollision={state.showCollision}
            showEncounters={state.showEncounters}
            showNpcOverlay={state.showNpcOverlay}
            showScriptOverlay={state.showScriptOverlay}
            onViewChange={(offsetX, offsetY) =>
              dispatch({ type: "SET_OFFSET", offsetX, offsetY })
            }
            onWatchZoom={(zoom) => dispatch({ type: "SET_ZOOM", zoom })}
            onPushUndo={() => dispatch({ type: "PUSH_UNDO" })}
            onPaintBlock={(col, row) => dispatch({ type: "PAINT_BLOCK", col, row })}
            onPaintCollision={(col, row) => dispatch({ type: "PAINT_COLLISION", col, row })}
            onPaintEncounter={(col, row) => dispatch({ type: "PAINT_ENCOUNTER", col, row })}
            onPasteAt={(x, y) => dispatch({ type: "PASTE_AT", x, y })}
            onSetSelection={(rect) => dispatch({ type: "SET_SELECTION", rect })}
            onClickNpc={(col, row) => dispatch({ type: "CLICK_NPC", col, row })}
            onClickScript={(col, row) => dispatch({ type: "CLICK_SCRIPT", col, row })}
            onOpenScript={(id) => dispatch({ type: "OPEN_SCRIPT", id })}
            onRightClick={(col, row) => dispatch({ type: "RIGHT_CLICK", col, row })}
          />
        </div>

        {/* status + compile output */}
        <div
          className="border-t border-neutral-800 bg-neutral-800/80"
          style={{ padding: "4px 10px", fontSize: 12 }}
        >
          status: {statusRef.current || state.status || "ready"}
        </div>
        {state.compileResult && (
          <div
            className="border-t border-neutral-800 bg-neutral-900"
            style={{
              padding: "6px 10px",
              fontSize: 12,
              maxHeight: 120,
              overflow: "auto",
            }}
            data-testid="compile-result"
          >
            {state.compileResult.ok ? (
              <div className="text-green-400">
                compile ok — {state.compileResult.artifacts.length} artifacts
                <ul>
                  {state.compileResult.artifacts.map((a) => (
                    <li key={a}>{a}</li>
                  ))}
                </ul>
              </div>
            ) : (
              <ul className="text-red-400" style={{ margin: 0, paddingLeft: 18 }}>
                {state.compileResult.errors.map((e, i) => (
                  <li key={i}>
                    line {e.line}: {e.msg}
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {/* inline script editor for selected object */}
        {selectedNpcs[0] && (
          <ScriptPane
            title={`NPC ${selectedNpcs[0].id} @ (${selectedNpcs[0].x},${selectedNpcs[0].y}) — script`}
            script={selectedNpcs[0].script}
            parseKey={state.parseKey === `npc:${selectedNpcs[0].id}` ? state.parseKey : null}
            parseResult={
              state.parseKey === `npc:${selectedNpcs[0].id}` ? state.parseResult : null
            }
            onChange={(text) =>
              dispatch({
                type: "SET_SCRIPT_TEXT",
                target: "npc",
                id: selectedNpcs[0].id,
                text,
              })
            }
          />
        )}
        {editingScripts[0] && (
          <ScriptPane
            title={`Script ${editingScripts[0].id} @ (${editingScripts[0].x},${editingScripts[0].y}) — script`}
            script={editingScripts[0].script}
            parseKey={state.parseKey === `script:${editingScripts[0].id}` ? state.parseKey : null}
            parseResult={
              state.parseKey === `script:${editingScripts[0].id}` ? state.parseResult : null
            }
            onChange={(text) =>
              dispatch({
                type: "SET_SCRIPT_TEXT",
                target: "script",
                id: editingScripts[0].id,
                text,
              })
            }
          />
        )}
      </div>
    </div>
  );
}