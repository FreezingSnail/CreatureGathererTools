// App shell: tab manager (Map / Creature / Move) + first-run setup dialog +
// header toolbar. All three editors stay mounted; inactive tabs are hidden
// with CSS so each editor keeps its own state across tab switches.
// MapEditor is remounted only on config reload (keyed by bootKey).

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MapEditor, type EditorDeps } from "./editor";
import { CreatureEditor, LayoutView, MoveEditor } from "./components";
import { SetupDialog } from "./SetupDialog";
import { emptyMap } from "./state";
import type { Config, GameMap, CompileResult, ParseResult } from "./types";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "./components/ui/tabs";
import { Button } from "./components/ui/button";
import { Alert } from "./components/ui/alert";
import { Card, CardContent } from "./components/ui/card";

const readBytes = async (path: string): Promise<Uint8Array> =>
  new Uint8Array(await invoke<number[]>("read_file_bytes", { path }));

type TabId = "map" | "creature" | "move" | "layout";

const TABS: { id: TabId; label: string }[] = [
  { id: "map", label: "Map" },
  { id: "creature", label: "Creature" },
  { id: "move", label: "Move" },
  { id: "layout", label: "Layout" },
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
      <main className="flex h-screen flex-col bg-background text-foreground">
        <p className="px-2.5 py-1 text-sm">loading…</p>
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
    <main className="flex h-screen flex-col bg-background text-foreground">
      <Tabs
        value={tab}
        onValueChange={(v) => setTab(v as TabId)}
        className="flex min-h-0 flex-1 flex-col"
      >
        <header className="flex flex-wrap items-center gap-4 border-b px-2.5 py-1.5">
          <span className="whitespace-nowrap text-[13px] font-semibold">
            CreatureGathererFX
          </span>
          <TabsList variant="line" aria-label="editors">
            {TABS.map((t) => (
              <TabsTrigger key={t.id} value={t.id}>
                {t.label}
              </TabsTrigger>
            ))}
          </TabsList>
          <div className="ml-auto flex gap-1.5">
            <Button size="sm" variant="outline" onClick={saveMap}>
              Save map
            </Button>
            <Button size="sm" variant="outline" onClick={compileAll}>
              Compile all
            </Button>
            <Button size="sm" variant="outline" onClick={reloadConfig}>
              Reload config
            </Button>
          </div>
        </header>

        {headerStatus && (
          <Alert
            variant={headerStatus.error ? "destructive" : "default"}
            role="status"
            className="shrink-0 rounded-none border-x-0 border-t-0 px-2.5 py-1.5 text-xs"
          >
            {headerStatus.text}
          </Alert>
        )}

        {compileResult && (
          <Card
            data-testid="app-compile-result"
            className="max-h-[140px] shrink-0 gap-0 overflow-auto rounded-none border-x-0 border-t-0 px-0 py-0 text-xs shadow-none"
          >
            <CardContent className="px-2.5 py-1.5">
              {compileResult.ok ? (
                <div>
                  <p className="text-muted-foreground">
                    compile ok — {compileResult.artifacts.length} artifacts
                  </p>
                  <ul className="list-disc pl-5">
                    {compileResult.artifacts.map((a) => (
                      <li key={a}>{a}</li>
                    ))}
                  </ul>
                </div>
              ) : (
                <ul className="list-disc pl-5">
                  {compileResult.errors.map((e, i) => (
                    <li key={i}>
                      line {e.line}: {e.msg}
                    </li>
                  ))}
                </ul>
              )}
            </CardContent>
          </Card>
        )}

        <div className="flex min-h-0 flex-1 flex-col">
          <TabsContent
            value="map"
            forceMount
            id="tab-panel-map"
            data-testid="tab-panel-map"
            aria-label="map"
            className="min-h-0 flex-1"
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
          </TabsContent>
          <TabsContent
            value="creature"
            forceMount
            id="tab-panel-creature"
            data-testid="tab-panel-creature"
            aria-label="creature"
            className="min-h-0 flex-1"
          >
            <CreatureEditor key={bootKey} />
          </TabsContent>
          <TabsContent
            value="move"
            forceMount
            id="tab-panel-move"
            data-testid="tab-panel-move"
            aria-label="move"
            className="min-h-0 flex-1"
          >
            <MoveEditor key={bootKey} />
          </TabsContent>
          <TabsContent
            value="layout"
            forceMount
            id="tab-panel-layout"
            data-testid="tab-panel-layout"
            aria-label="layout"
            className="min-h-0 flex-1"
          >
            <LayoutView key={bootKey} />
          </TabsContent>
        </div>
      </Tabs>
    </main>
  );
}

export default App;