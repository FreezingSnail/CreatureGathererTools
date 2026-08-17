// App shell: tab manager (Map / Creature / Move) + first-run setup dialog +
// header toolbar. All three editors stay mounted; inactive tabs are hidden
// with CSS so each editor keeps its own state across tab switches.
// MapEditor is remounted only on config reload (keyed by bootKey).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MapEditor, type EditorDeps } from "./editor";
import { CreatureEditor, MoveEditor } from "./components";
import { SetupDialog } from "./SetupDialog";
import { emptyMap } from "./state";
import type { Config, GameMap, CompileResult, ParseResult } from "./types";
import "./app.css";

const readBytes = async (path: string): Promise<Uint8Array> =>
  new Uint8Array(await invoke<number[]>("read_file_bytes", { path }));

type TabId = "map" | "creature" | "move";

const TABS: { id: TabId; label: string }[] = [
  { id: "map", label: "Map" },
  { id: "creature", label: "Creature" },
  { id: "move", label: "Move" },
];

type Phase = "checking" | "setup" | "editors";

const emptyConfig = (): Config => ({
  creatures_json: "",
  moves_json: "",
  map_json: "",
  tileset_png: "",
  map_tileset_png: "",
  npc_tileset_png: "",
  output_dir: "",
});

function App() {
  const [phase, setPhase] = useState<Phase>("checking");
  const [config, setConfig] = useState<Config | null>(null);
  const [map, setMap] = useState<GameMap | null>(null);
  const [tab, setTab] = useState<TabId>("map");
  const [bootKey, setBootKey] = useState(0); // remount editors on config reload
  const [setupError, setSetupError] = useState<string | null>(null);
  const [headerStatus, setHeaderStatus] = useState<{
    text: string;
    error: boolean;
  } | null>(null);
  const [compileResult, setCompileResult] = useState<CompileResult | null>(null);
  const mapSaveRef = useRef<(() => void) | null>(null);

  const deps = useMemo<EditorDeps>(
    () => ({
      readBytes,
      saveMap: (m) => invoke("save_map", { map: m }),
      compileMap: () => invoke<CompileResult>("compile_map"),
      parseScript: (src) => invoke<ParseResult>("parse_script", { src }),
    }),
    []
  );

  // Load config + map on mount. Missing config (load_config error) → setup.
  const boot = useCallback(async () => {
    setSetupError(null);
    try {
      const cfg = await invoke<Config>("load_config");
      let m: GameMap;
      try {
        m = await invoke<GameMap>("load_map");
        setHeaderStatus(null);
      } catch (mapErr) {
        m = emptyMap(16, 16);
        setHeaderStatus({
          text: `map failed to load, editing empty: ${String(mapErr)}`,
          error: true,
        });
      }
      setConfig(cfg);
      setMap(m);
      setPhase("editors");
    } catch (e) {
      setSetupError(String(e));
      setPhase("setup");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    boot();
  }, [boot]);

  const submitSetup = async (cfg: Config) => {
    setSetupError(null);
    try {
      await invoke("save_config", { config: cfg });
      let m: GameMap;
      try {
        m = await invoke<GameMap>("load_map");
        setHeaderStatus(null);
      } catch (mapErr) {
        m = emptyMap(16, 16);
        setHeaderStatus({
          text: `map failed to load, editing empty: ${String(mapErr)}`,
          error: true,
        });
      }
      setConfig(cfg);
      setMap(m);
      setBootKey((k) => k + 1);
      setPhase("editors");
    } catch (e) {
      setSetupError(`save failed: ${String(e)}`);
    }
  };

  const reloadConfig = async () => {
    setHeaderStatus(null);
    setCompileResult(null);
    try {
      const cfg = await invoke<Config>("load_config");
      let m: GameMap;
      try {
        m = await invoke<GameMap>("load_map");
      } catch (mapErr) {
        m = emptyMap(16, 16);
        setHeaderStatus({
          text: `map failed to load, editing empty: ${String(mapErr)}`,
          error: true,
        });
      }
      setConfig(cfg);
      setMap(m);
      setBootKey((k) => k + 1);
    } catch (e) {
      setSetupError(String(e));
      setPhase("setup");
    }
  };

  const saveMap = () => {
    if (mapSaveRef.current) {
      mapSaveRef.current();
    } else {
      setHeaderStatus({ text: "map editor not ready", error: true });
    }
  };

  const compileAll = async () => {
    setHeaderStatus({ text: "compiling…", error: false });
    try {
      const result = await invoke<CompileResult>("compile_all");
      setCompileResult(result);
      setHeaderStatus({
        text: result.ok
          ? "compile ok"
          : `compile failed (${result.errors.length} errors)`,
        error: !result.ok,
      });
    } catch (e) {
      setCompileResult(null);
      setHeaderStatus({ text: `compile error: ${String(e)}`, error: true });
    }
  };

  if (phase === "checking") {
    return (
      <main className="cgfx-app">
        <p className="cgfx-app__status">loading…</p>
      </main>
    );
  }

  if (phase === "setup") {
    return (
      <>
        <SetupDialog initial={config ?? emptyConfig()} error={setupError ?? undefined} onSubmit={submitSetup} />
      </>
    );
  }

  return (
    <main className="cgfx-app">
      <header className="cgfx-app__header">
        <span className="cgfx-app__title">CreatureGathererFX</span>
        <div className="cgfx-app__tabs" role="tablist" aria-label="editors">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              aria-controls={`tab-panel-${t.id}`}
              className={`cgfx-app__tab${tab === t.id ? " cgfx-app__tab--active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <div className="cgfx-app__actions">
          <button type="button" onClick={saveMap}>
            Save map
          </button>
          <button type="button" onClick={compileAll}>
            Compile all
          </button>
          <button type="button" onClick={reloadConfig}>
            Reload config
          </button>
        </div>
      </header>

      {headerStatus && (
        <div
          className={`cgfx-app__status${headerStatus.error ? " cgfx-app__status--error" : ""}`}
          role="status"
        >
          {headerStatus.text}
        </div>
      )}

      {compileResult && (
        <div className="cgfx-app__compile" data-testid="app-compile-result">
          {compileResult.ok ? (
            <div>
              compile ok — {compileResult.artifacts.length} artifacts
              <ul>
                {compileResult.artifacts.map((a) => (
                  <li key={a}>{a}</li>
                ))}
              </ul>
            </div>
          ) : (
            <ul>
              {compileResult.errors.map((e, i) => (
                <li key={i}>
                  line {e.line}: {e.msg}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <div className="cgfx-app__body">
        <section
          id="tab-panel-map"
          role="tabpanel"
          aria-label="map"
          data-testid="tab-panel-map"
          className="cgfx-app__panel"
          style={{ display: tab === "map" ? undefined : "none" }}
        >
          {config && map && (
            <MapEditor
              key={bootKey}
              config={config}
              map={map}
              deps={deps}
              active={tab === "map"}
              saveRef={mapSaveRef}
            />
          )}
        </section>
        <section
          id="tab-panel-creature"
          role="tabpanel"
          aria-label="creature"
          data-testid="tab-panel-creature"
          className="cgfx-app__panel"
          style={{ display: tab === "creature" ? undefined : "none" }}
        >
          <CreatureEditor key={bootKey} />
        </section>
        <section
          id="tab-panel-move"
          role="tabpanel"
          aria-label="move"
          data-testid="tab-panel-move"
          className="cgfx-app__panel"
          style={{ display: tab === "move" ? undefined : "none" }}
        >
          <MoveEditor key={bootKey} />
        </section>
      </div>
    </main>
  );
}

export default App;