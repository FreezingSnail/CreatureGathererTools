// Map editor assembly: toolbar + palette sidebar + canvas + inline script
// editor + save/compile. Drives the Tauri command layer via injected async
// functions so components stay testable outside the webview.

import {
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
  type MutableRefObject,
} from "react";
import type { Config, GameMap, ParseResult, CompileResult } from "./types";
import {
  initialState,
  reducer,
  LAYERS,
  type LayerType,
} from "./state";
import {
  Tileset,
  loadMapTileset,
  loadTilesetFromBytes,
  emptyTileset,
} from "./tileset";
import { MapCanvas } from "./canvas";
import { Palette } from "./palette";
import { ScriptPane } from "./scriptPane";

export interface EditorDeps {
  readBytes: (path: string) => Promise<Uint8Array>;
  saveMap: (map: GameMap) => Promise<void>;
  compileMap: () => Promise<CompileResult>;
  parseScript: (src: string) => Promise<ParseResult>;
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
      : state.selectedScript >= 0
        ? `script:${state.selectedScript}`
        : null;
  const selectedScriptText =
    state.selectedNpc >= 0 && state.map.npcs[state.selectedNpc]
      ? state.map.npcs[state.selectedNpc].script
      : state.selectedScript >= 0 && state.map.scripts[state.selectedScript]
        ? state.map.scripts[state.selectedScript].script
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

  // keyboard shortcuts: Ctrl+Z undo, Ctrl+= zoom in, Ctrl+- zoom out, H highlight
  useEffect(() => {
    if (!active) {
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        dispatch({ type: "UNDO" });
      } else if ((e.ctrlKey || e.metaKey) && e.key === "=") {
        e.preventDefault();
        dispatch({ type: "SET_ZOOM", zoom: state.zoom * 1.5 });
      } else if ((e.ctrlKey || e.metaKey) && e.key === "-") {
        e.preventDefault();
        dispatch({ type: "SET_ZOOM", zoom: state.zoom / 1.5 });
      } else if (e.key.toLowerCase() === "h") {
        dispatch({ type: "TOGGLE_CHUNK_HIGHLIGHT" });
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
      await deps.saveMap(state.map);
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

  const selectedNpcs = state.selectedNpc >= 0 ? [state.map.npcs[state.selectedNpc]] : [];
  const selectedScripts =
    state.selectedScript >= 0 ? [state.map.scripts[state.selectedScript]] : [];

  const toggle = (label: string, checked: boolean, onChange: () => void) => (
    <label style={{ fontSize: 12, display: "inline-flex", alignItems: "center", gap: 3 }}>
      <input type="checkbox" checked={checked} onChange={onChange} />
      {label}
    </label>
  );

  return (
    <div style={{ display: "flex", height: "100%", fontFamily: "system-ui, sans-serif" }}>
      {/* sidebar */}
      <div
        style={{
          width: 230,
          borderRight: "1px solid #333",
          display: "flex",
          flexDirection: "column",
          background: "#171717",
        }}
      >
        <div style={{ display: "flex", flexWrap: "wrap", gap: 4, padding: 6 }}>
          {(["map", "npc", "collision", "encounter"] as PaletteTab[]).map((t) => (
            <button
              key={t}
              onClick={() => {
                setPaletteTab(t);
                dispatch({ type: "SET_LAYER", layer: t as LayerType });
              }}
              style={{
                fontSize: 12,
                padding: "3px 8px",
                background: paletteTab === t ? "#316dca" : "#222",
                color: "#ddd",
                border: "1px solid #444",
                borderRadius: 3,
                cursor: "pointer",
              }}
            >
              {t}
            </button>
          ))}
        </div>
        <div style={{ flex: 1, overflow: "auto", padding: 4 }}>
          {paletteTab === "map" && (
            <Palette
              tiles={mapTiles.tiles}
              perRow={mapTiles.perRow}
              selected={state.activeBlock}
              onSelectBlock={(block, w, h) =>
                dispatch({ type: "SET_ACTIVE_BLOCK", block, w, h })
              }
              emptyLabel="No map tileset loaded (set map_tileset_png)"
            />
          )}
          {paletteTab === "npc" && (
            <Palette
              tiles={npcTiles.tiles}
              perRow={npcTiles.perRow}
              selected={[state.activeNpcSprite]}
              onSelectBlock={(block) =>
                dispatch({ type: "SET_NPC_SPRITE", sprite: block[0] ?? 0 })
              }
              emptyLabel="No NPC tileset loaded (set npc_tileset_png)"
            />
          )}
          {paletteTab === "collision" && (
            <div style={{ padding: 8, display: "flex", flexDirection: "column", gap: 8 }}>
              <label style={{ fontSize: 12 }}>
                Active collision type
                <select
                  value={state.activeCollisionType}
                  onChange={(e) =>
                    dispatch({ type: "SET_COLLISION_TYPE", id: Number(e.target.value) })
                  }
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
                <span style={{ fontSize: 11, color: "#888" }}>
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
                  onChange={(e) =>
                    dispatch({ type: "SET_ENCOUNTER_ZONE", id: Number(e.target.value) })
                  }
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
                <span style={{ fontSize: 11, color: "#888" }}>
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
        <div style={{ padding: 6, fontSize: 11, color: "#888", borderTop: "1px solid #333" }}>
          wheel: zoom · shift+wheel: pan · drag: paint · alt/middle-drag: pan ·
          right-click: delete/clear · ctrl+z: undo
        </div>
      </div>

      {/* main column */}
      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
        {/* toolbar */}
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "center",
            padding: "6px 10px",
            borderBottom: "1px solid #333",
            flexWrap: "wrap",
            background: "#1f1f1f",
          }}
        >
          <span style={{ fontSize: 12 }}>Layer:</span>
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
          <button onClick={() => dispatch({ type: "SET_BRUSH_SIZE", size: state.brushSize - 1 })}>
            -
          </button>
          <span style={{ fontSize: 12 }}>brush {state.brushSize}</span>
          <button onClick={() => dispatch({ type: "SET_BRUSH_SIZE", size: state.brushSize + 1 })}>
            +
          </button>
          <button disabled={state.undoStack.length === 0} onClick={() => dispatch({ type: "UNDO" })}>
            undo
          </button>
          <button onClick={() => dispatch({ type: "SET_ZOOM", zoom: state.zoom / 1.5 })}>-Z</button>
          <span style={{ fontSize: 12 }}>{Math.round(state.zoom * 10) / 10}x</span>
          <button onClick={() => dispatch({ type: "SET_ZOOM", zoom: state.zoom * 1.5 })}>+Z</button>
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
            <button onClick={save} style={{ background: "#2a5c2a" }}>
              Save
            </button>
            <button onClick={compile} style={{ background: "#5c4a2a" }}>
              Compile
            </button>
          </span>
        </div>

        {/* canvas */}
        <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
          <MapCanvas
            map={state.map}
            tiles={mapTiles.tiles}
            npcTiles={npcTiles.tiles}
            layer={state.layer}
            zoom={state.zoom}
            offsetX={state.offsetX}
            offsetY={state.offsetY}
            activeBlock={state.activeBlock}
            blockW={state.blockW}
            blockH={state.blockH}
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
            onClickNpc={(col, row) => dispatch({ type: "CLICK_NPC", col, row })}
            onClickScript={(col, row) => dispatch({ type: "CLICK_SCRIPT", col, row })}
            onRightClick={(col, row) => dispatch({ type: "RIGHT_CLICK", col, row })}
          />
        </div>

        {/* status + compile output */}
        <div style={{ borderTop: "1px solid #333", padding: "4px 10px", fontSize: 12, background: "#1f1f1f" }}>
          status: {statusRef.current || state.status || "ready"}
        </div>
        {state.compileResult && (
          <div
            style={{
              borderTop: "1px solid #333",
              padding: "6px 10px",
              fontSize: 12,
              maxHeight: 120,
              overflow: "auto",
              background: "#181818",
            }}
            data-testid="compile-result"
          >
            {state.compileResult.ok ? (
              <div style={{ color: "#6f6" }}>
                compile ok — {state.compileResult.artifacts.length} artifacts
                <ul>
                  {state.compileResult.artifacts.map((a) => (
                    <li key={a}>{a}</li>
                  ))}
                </ul>
              </div>
            ) : (
              <ul style={{ color: "#f66", margin: 0, paddingLeft: 18 }}>
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
        {selectedScripts[0] && (
          <ScriptPane
            title={`Script ${selectedScripts[0].id} @ (${selectedScripts[0].x},${selectedScripts[0].y}) — script`}
            script={selectedScripts[0].script}
            parseKey={state.parseKey === `script:${selectedScripts[0].id}` ? state.parseKey : null}
            parseResult={
              state.parseKey === `script:${selectedScripts[0].id}` ? state.parseResult : null
            }
            onChange={(text) =>
              dispatch({
                type: "SET_SCRIPT_TEXT",
                target: "script",
                id: selectedScripts[0].id,
                text,
              })
            }
          />
        )}
      </div>
    </div>
  );
}