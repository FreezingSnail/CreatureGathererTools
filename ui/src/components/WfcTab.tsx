import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Block, Config, GameMap, WfcPiece, WfcProject } from "../types";
import { loadMapTileset } from "../tileset";
import { extractThumbnailTiles, removeWfcPieces, slotSizeMigrationOffenders } from "../wfc";
import { Button } from "./ui/button";
import { Input } from "./ui/input";
import { PieceSources } from "./PieceSources";
import { RecipePanel } from "./RecipePanel";

const DEFAULT_PROJECT: WfcProject = {
  version: 1,
  slot_w: 2,
  slot_h: 2,
  pieces: [],
  socket_labels: [],
  recipes: [],
};
const STAGES = ["pieces", "recipe", "generate", "advanced"] as const;
type Stage = typeof STAGES[number];
const EMPTY_TILES: HTMLCanvasElement[] = [];
const EMPTY_BLOCKS: Block[] = [];

export interface WfcTabProps {
  map: GameMap;
  config?: Config;
  tiles?: HTMLCanvasElement[];
  /** Project block library used as piece input. */
  blocks?: Block[];
  /** Active tileset columns; required for correct row-major rectangle capture. */
  perRow?: number;
  onMapChange?: (map: GameMap) => void;
  saveMap?: (map: GameMap) => Promise<void>;
}

function PieceThumbnail({ piece, tiles }: { piece: WfcPiece; tiles: HTMLCanvasElement[] }) {
  const values = extractThumbnailTiles(piece);
  const scale = Math.max(1, Math.min(12, Math.floor(96 / Math.max(piece.tw, piece.th, 1))));
  return (
    <div className="flex min-h-20 min-w-20 items-center justify-center rounded border bg-muted/30 p-1">
      <canvas
        width={Math.max(1, piece.tw * scale)}
        height={Math.max(1, piece.th * scale)}
        aria-label={`${piece.name} thumbnail`}
        ref={(canvas) => {
          const context = canvas?.getContext("2d");
          if (!canvas || !context) return;
          context.clearRect(0, 0, canvas.width, canvas.height);
          values.forEach((tile, index) => {
            const source = tiles[tile];
            if (source) context.drawImage(source, (index % piece.tw) * scale, Math.floor(index / piece.tw) * scale, scale, scale);
          });
        }}
        style={{ imageRendering: "pixelated" }}
      />
    </div>
  );
}

function projectWithDefaults(value: WfcProject | undefined): WfcProject {
  return value ? {
    ...value,
    pieces: (value.pieces ?? []).map((piece) => ({ ...piece, edge_overrides: piece.edge_overrides ?? [], tags: piece.tags ?? [] })),
    socket_labels: value.socket_labels ?? [],
    recipes: (value.recipes ?? []).map((recipe) => ({ ...recipe, anchors: recipe.anchors ?? [], socket_compat: recipe.socket_compat ?? [], rules: recipe.rules ?? [] })),
  } : { ...DEFAULT_PROJECT };
}

export function WfcTab({ map, config, tiles: suppliedTiles = EMPTY_TILES, blocks = EMPTY_BLOCKS, perRow = 8, onMapChange, saveMap }: WfcTabProps) {
  const [project, setProject] = useState(() => projectWithDefaults(map.wfc));
  const [stage, setStage] = useState<Stage>("pieces");
  const [selectedId, setSelectedId] = useState<number | null>(() => map.wfc?.pieces?.[0]?.id ?? null);
  const [tiles, setTiles] = useState(suppliedTiles);
  const [sheetPerRow, setSheetPerRow] = useState(Math.max(1, perRow));
  const [validation, setValidation] = useState<string[]>([]);
  const [slotError, setSlotError] = useState<string | null>(null);
  const [weightErrors, setWeightErrors] = useState<Record<number, string>>({});
  const selected = project.pieces.find((piece) => piece.id === selectedId) ?? project.pieces[0] ?? null;
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    setProject(projectWithDefaults(map.wfc));
    setSelectedId(map.wfc?.pieces?.[0]?.id ?? null);
  }, [map.wfc]);

  useEffect(() => {
    setTiles(suppliedTiles);
    setSheetPerRow(Math.max(1, perRow));
  }, [suppliedTiles, perRow]);

  useEffect(() => {
    if (!config || suppliedTiles.length || !config.map_tileset_png && !config.tileset_png) return;
    let cancelled = false;
    loadMapTileset(
      async (path) => new Uint8Array(await invoke<number[]>("read_file_bytes", { path })),
      config.map_tileset_png,
      config.tileset_png,
    ).then((loaded) => {
      if (!cancelled) {
        setTiles(loaded.tiles);
        setSheetPerRow(Math.max(1, loaded.perRow));
      }
    });
    return () => { cancelled = true; };
  }, [config, suppliedTiles]);

  useEffect(() => {
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      invoke<string[]>("wfc_validate", { project }).then(setValidation).catch(() => setValidation([]));
    }, 350);
    return () => window.clearTimeout(saveTimer.current);
  }, [project]);

  const persist = (next: WfcProject) => {
    const nextMap = { ...map, wfc: next };
    setProject(next);
    onMapChange?.(nextMap);
    saveMap?.(nextMap).catch(() => undefined);
  };

  const updatePiece = (id: number, update: Partial<WfcPiece>) => {
    persist({ ...project, pieces: project.pieces.map((piece) => piece.id === id ? { ...piece, ...update } : piece) });
  };

  const rename = (piece: WfcPiece) => {
    const name = window.prompt("Piece name", piece.name)?.trim();
    if (name && name !== piece.name) updatePiece(piece.id, { name });
  };

  const remove = (piece: WfcPiece) => {
    const recipes = project.recipes.filter((recipe) => recipe.piece_ids.includes(piece.id));
    const suffix = recipes.length ? `\nReferenced by: ${recipes.map((recipe) => recipe.name || `recipe ${recipe.id}`).join(", ")}` : "";
    if (!window.confirm(`Delete piece ${piece.name}?${suffix}`)) return;
    persist(removeWfcPieces(project, [piece.id]));
    if (selectedId === piece.id) setSelectedId(null);
  };

  const removeAll = () => {
    if (!project.pieces.length || !window.confirm(`Delete all ${project.pieces.length} WFC pieces? This clears recipe piece selections, anchors, and rules that reference them.`)) return;
    persist(removeWfcPieces(project, project.pieces.map((piece) => piece.id)));
    setSelectedId(null);
  };

  const changeWeight = (piece: WfcPiece, raw: string) => {
    const value = Number(raw);
    if (!Number.isFinite(value) || value <= 0) {
      setWeightErrors((current) => ({ ...current, [piece.id]: "Weight must be greater than zero." }));
      return;
    }
    setWeightErrors((current) => { const next = { ...current }; delete next[piece.id]; return next; });
    updatePiece(piece.id, { weight: value });
  };

  const changeSlots = (slotW: number, slotH: number) => {
    const offenders = slotSizeMigrationOffenders(project, slotW, slotH);
    if (offenders.length) {
      setSlotError(`Cannot use ${slotW}×${slotH}: changing slots would reinterpret pieces. Offending pieces: ${offenders.map((piece) => piece.name || `piece ${piece.id}`).join(", ")}.`);
      return;
    }
    setSlotError(null);
    persist({ ...project, slot_w: slotW, slot_h: slotH });
  };

  const addPieces = (created: WfcPiece[]) => {
    if (!created.length) return;
    persist({ ...project, pieces: [...project.pieces, ...created] });
    setSelectedId(created[created.length - 1]!.id);
  };

  return (
    <div data-testid="wfc-tab" className="h-full overflow-auto p-3 text-sm">
      <div className="mb-3 flex flex-wrap items-center gap-3 border-b pb-3">
        <strong className="text-base">WFC workflow</strong>
        <label className="flex items-center gap-1 text-xs">Slot W
          <Input aria-label="WFC slot width" className="w-16" type="number" min="1" value={project.slot_w} onChange={(event) => changeSlots(Math.max(1, Math.floor(Number(event.target.value) || 1)), project.slot_h)} />
        </label>
        <label className="flex items-center gap-1 text-xs">Slot H
          <Input aria-label="WFC slot height" className="w-16" type="number" min="1" value={project.slot_h} onChange={(event) => changeSlots(project.slot_w, Math.max(1, Math.floor(Number(event.target.value) || 1)))} />
        </label>
        <span className="text-xs text-muted-foreground">Pieces must align to this project grid; Blocks do not.</span>
      </div>
      <nav aria-label="WFC workflow stages" className="mb-3 flex flex-wrap gap-1" role="tablist">
        {STAGES.map((item, index) => <button key={item} type="button" role="tab" aria-selected={stage === item} aria-controls={`wfc-stage-${item}`} id={`wfc-tab-${item}`} className={`rounded border px-3 py-1.5 ${stage === item ? "border-primary bg-primary/10 font-medium" : "hover:bg-muted"}`} onClick={() => setStage(item)} onKeyDown={(event) => { if (event.key !== "ArrowRight" && event.key !== "ArrowLeft" && event.key !== "Home" && event.key !== "End") return; event.preventDefault(); const next = event.key === "Home" ? 0 : event.key === "End" ? STAGES.length - 1 : (index + (event.key === "ArrowRight" ? 1 : -1) + STAGES.length) % STAGES.length; setStage(STAGES[next]!); }}>{item[0]!.toUpperCase() + item.slice(1)}</button>)}
      </nav>
      <h1 className="mb-2 text-lg font-semibold">{stage === "pieces" ? "Pieces" : stage === "recipe" ? "Recipe" : stage === "generate" ? "Generate" : "Advanced"}</h1>
      <div role="tabpanel" id={`wfc-stage-${stage}`} aria-labelledby={`wfc-tab-${stage}`}>
        {slotError && <div role="alert" className="mb-3 rounded border border-destructive p-2 text-destructive">{slotError}</div>}
        {stage === "pieces" && <>
          <PieceSources blocks={blocks} pieces={project.pieces} tiles={tiles} perRow={sheetPerRow} slotW={project.slot_w} slotH={project.slot_h} onAddPieces={addPieces} />
          <section aria-label="Piece library" className="rounded-lg border p-2">
            <div className="mb-2 flex flex-wrap items-center justify-between gap-2"><h2 className="font-medium">Piece library</h2><Button type="button" size="sm" variant="destructive" disabled={!project.pieces.length} onClick={removeAll}>Delete all pieces</Button></div>
            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {project.pieces.map((piece) => <article key={piece.id} className={`rounded-lg border p-2 ${selected?.id === piece.id ? "border-primary" : ""}`}>
              <button type="button" className="w-full text-left" onClick={() => setSelectedId(piece.id)}><PieceThumbnail piece={piece} tiles={tiles} /><div className="mt-1 font-medium">{piece.name || `Piece ${piece.id}`}</div><div className="text-xs text-muted-foreground">{piece.tw}×{piece.th} tiles · {Math.floor(piece.tw / project.slot_w)}×{Math.floor(piece.th / project.slot_h)} slots</div></button>
              <div className="mt-2 grid grid-cols-2 gap-2"><label>Weight<input aria-label={`${piece.name} weight`} type="number" min="0.0001" step="any" value={piece.weight} onChange={(event) => changeWeight(piece, event.target.value)} aria-invalid={Boolean(weightErrors[piece.id])} /></label><label>Limit<input aria-label={`${piece.name} limit`} type="number" min="0" step="1" value={piece.limit ?? ""} onChange={(event) => updatePiece(piece.id, { limit: event.target.value === "" ? null : Math.max(0, Number(event.target.value)) })} /></label></div>
              {weightErrors[piece.id] && <div role="alert" className="text-xs text-destructive">{weightErrors[piece.id]}</div>}<div className="mt-2 flex gap-1"><Button type="button" size="xs" variant="outline" onClick={() => rename(piece)}>Rename</Button><Button type="button" size="xs" variant="destructive" onClick={() => remove(piece)}>Delete</Button></div>
            </article>)}
            {!project.pieces.length && <p className="rounded border border-dashed p-4 text-muted-foreground">No pieces yet. Import blocks from the project library above, or add sheet fill pieces.</p>}
            </div>
          </section>
        </>}
        {stage !== "pieces" && <RecipePanel mode={stage} project={project} pieces={project.pieces} tiles={tiles} map={map} validation={validation} onProjectChange={persist} onMapChange={onMapChange} saveMap={saveMap} />}
      </div>
    </div>
  );
}

export default WfcTab;
