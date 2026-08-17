import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { MapEditor, type EditorDeps } from "./editor";
import type { Config, GameMap, ParseResult, CompileResult } from "./types";

const readBytes = async (path: string): Promise<Uint8Array> =>
  new Uint8Array(await invoke<number[]>("read_file_bytes", { path }));

function App() {
  const [boot, setBoot] = useState<{ config: Config; map: GameMap } | null>(
    null
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const config = await invoke<Config>("load_config");
        const map = await invoke<GameMap>("load_map");
        setBoot({ config, map });
      } catch (err) {
        setError(String(err));
      }
    })();
  }, []);

  const deps: EditorDeps = {
    readBytes,
    saveMap: (m) => invoke("save_map", { map: m }),
    compileMap: () => invoke<CompileResult>("compile_map"),
    parseScript: (src) => invoke<ParseResult>("parse_script", { src }),
  };

  return (
    <main style={{ height: "100vh", display: "flex", flexDirection: "column" }}>
      <h1
        style={{
          fontSize: 14,
          margin: 0,
          padding: "6px 10px",
          borderBottom: "1px solid #333",
          background: "#222",
          color: "#ddd",
        }}
      >
        CreatureGathererFX — Map Editor
      </h1>
      {error ? (
        <p style={{ padding: 10, color: "red" }}>
          failed to load config/map: {error}
        </p>
      ) : boot ? (
        <div style={{ flex: 1, minHeight: 0 }}>
          <MapEditor config={boot.config} map={boot.map} deps={deps} />
        </div>
      ) : (
        <p style={{ padding: 10 }}>loading…</p>
      )}
    </main>
  );
}

export default App;