import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Config, GameMap, SocketKind, WfcPiece, WfcProject } from "../types";
import { loadMapTileset } from "../tileset";
import {
  derivePieceSockets,
  extractThumbnailTiles,
  legalNeighbourCount,
  nextWfcId,
  slotSizeMigrationOffenders,
  socketKindLabel,
  type SocketDisplay,
} from "../wfc";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

const DEFAULT_PROJECT: WfcProject = {
  version: 1,
  slot_w: 2,
  slot_h: 2,
  pieces: [],
  socket_labels: [],
  recipes: [],
};
const SLOT_SIZES = [1, 2, 4] as const;
const EMPTY_TILES: HTMLCanvasElement[] = [];

export interface WfcTabProps {
  map: GameMap;
  config?: Config;
  tiles?: HTMLCanvasElement[];
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

function edgeText(edge: SocketDisplay): string {
  if (edge.kind === "label") return `label ${edge.label}`;
  if (edge.kind === "private") return "private internal (siblings only)";
  return `derived strip [${edge.tiles.join(", ")}]`;
}

function projectWithDefaults(value: WfcProject | undefined): WfcProject {
  return value ? {
    ...value,
    pieces: value.pieces ?? [],
    socket_labels: value.socket_labels ?? [],
    recipes: value.recipes ?? [],
  } : { ...DEFAULT_PROJECT };
}

export function WfcTab({ map, config, tiles: suppliedTiles = EMPTY_TILES, onMapChange, saveMap }: WfcTabProps) {
  const [project, setProject] = useState(() => projectWithDefaults(map.wfc));
  const [selectedId, setSelectedId] = useState<number | null>(() => map.wfc?.pieces?.[0]?.id ?? null);
  const [tiles, setTiles] = useState(suppliedTiles);
  const [validation, setValidation] = useState<string[]>([]);
  const [slotError, setSlotError] = useState<string | null>(null);
  const [weightErrors, setWeightErrors] = useState<Record<number, string>>({});
  const [labelDraft, setLabelDraft] = useState("");
  const [labelKind, setLabelKind] = useState<"symmetric" | "directed">("symmetric");
  const [partner, setPartner] = useState("");
  const [newCompat, setNewCompat] = useState<[string, string]>(["", ""]);
  const selected = project.pieces.find((piece) => piece.id === selectedId) ?? project.pieces[0] ?? null;
  const selectedEdges = useMemo(() => selected ? derivePieceSockets(selected, project.slot_w, project.slot_h) : [], [selected, project.slot_w, project.slot_h]);
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    setProject(projectWithDefaults(map.wfc));
    setSelectedId(map.wfc?.pieces?.[0]?.id ?? null);
  }, [map.wfc]);

  useEffect(() => {
    setTiles(suppliedTiles);
  }, [suppliedTiles]);

  useEffect(() => {
    if (!config || suppliedTiles.length || !config.map_tileset_png && !config.tileset_png) return;
    let cancelled = false;
    loadMapTileset(
      async (path) => new Uint8Array(await invoke<number[]>("read_file_bytes", { path })),
      config.map_tileset_png,
      config.tileset_png,
    ).then((loaded) => { if (!cancelled) setTiles(loaded.tiles); });
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
    persist({ ...project, pieces: project.pieces.filter((item) => item.id !== piece.id), recipes: project.recipes.map((recipe) => ({ ...recipe, piece_ids: recipe.piece_ids.filter((id) => id !== piece.id) })) });
    if (selectedId === piece.id) setSelectedId(null);
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

  const assignLabel = (edge: SocketDisplay, value: string) => {
    const overrides = project.pieces.find((piece) => piece.id === selected?.id)?.edge_overrides ?? [];
    const nextOverrides = overrides.filter((item) => !(item.side === edge.side && item.index === edge.index));
    if (value) nextOverrides.push({ side: edge.side, index: edge.index, label: Number(value) });
    if (selected) updatePiece(selected.id, { edge_overrides: nextOverrides });
  };

  const createLabel = () => {
    const name = labelDraft.trim();
    if (!name || (labelKind === "directed" && !partner)) return;
    const id = nextWfcId(project.socket_labels);
    const kind: SocketKind = labelKind === "directed" ? { directed: { partner: Number(partner) } } : "symmetric";
    persist({ ...project, socket_labels: [...project.socket_labels, { id, name, kind }] });
    setLabelDraft("");
    setPartner("");
  };

  const updateLabel = (id: number, update: { name?: string; kind?: SocketKind }) => {
    persist({ ...project, socket_labels: project.socket_labels.map((label) => label.id === id ? { ...label, ...update } : label) });
  };

  const deleteLabel = (id: number) => {
    persist({
      ...project,
      socket_labels: project.socket_labels.filter((label) => label.id !== id),
      pieces: project.pieces.map((piece) => ({ ...piece, edge_overrides: piece.edge_overrides.filter((edge) => edge.label !== id) })),
      recipes: project.recipes.map((recipe) => ({
        ...recipe,
        socket_compat: recipe.socket_compat.filter(([left, right]) => left !== id && right !== id),
      })),
    });
  };

  const addCompat = (recipeId: number) => {
    const left = Number(newCompat[0]);
    const right = Number(newCompat[1]);
    if (!left || !right) return;
    persist({ ...project, recipes: project.recipes.map((recipe) => recipe.id === recipeId ? { ...recipe, socket_compat: [...recipe.socket_compat, [left, right]] } : recipe) });
    setNewCompat(["", ""]);
  };

  const removeCompat = (recipeId: number, pairIndex: number) => {
    persist({
      ...project,
      recipes: project.recipes.map((recipe) => recipe.id === recipeId
        ? { ...recipe, socket_compat: recipe.socket_compat.filter((_, index) => index !== pairIndex) }
        : recipe),
    });
  };

  return (
    <div data-testid="wfc-tab" className="h-full overflow-auto p-3 text-sm">
      <div className="mb-3 flex flex-wrap items-center gap-3 border-b pb-3">
        <strong className="text-base">WFC pieces</strong>
        <label className="flex items-center gap-2">Slot size
          <select aria-label="Slot size" value={`${project.slot_w}x${project.slot_h}`} onChange={(event) => { const [w, h] = event.target.value.split("x").map(Number); changeSlots(w, h); }}>
            {SLOT_SIZES.flatMap((size) => <option key={`${size}x${size}`} value={`${size}x${size}`}>{size}×{size}</option>)}
          </select>
        </label>
        <span className="text-muted-foreground">Pieces are sliced at this project grid; changes never silently reinterpret captured tiles.</span>
      </div>
      {slotError && <div role="alert" className="mb-3 rounded border border-destructive p-2 text-destructive">{slotError}</div>}
      <div className="grid gap-3 xl:grid-cols-[minmax(0,2fr)_minmax(320px,1fr)]">
        <section aria-label="Piece library">
          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {project.pieces.map((piece) => (
              <article key={piece.id} className={`rounded-lg border p-2 ${selected?.id === piece.id ? "border-primary" : ""}`}>
                <button type="button" className="w-full text-left" onClick={() => setSelectedId(piece.id)}>
                  <PieceThumbnail piece={piece} tiles={tiles} />
                  <div className="mt-1 font-medium">{piece.name || `Piece ${piece.id}`}</div>
                  <div className="text-xs text-muted-foreground">{piece.tw}×{piece.th} tiles · {Math.floor(piece.tw / project.slot_w)}×{Math.floor(piece.th / project.slot_h)} slots</div>
                </button>
                <div className="mt-2 grid grid-cols-2 gap-2">
                  <label>Weight<input aria-label={`${piece.name} weight`} type="number" min="0.0001" step="any" value={piece.weight} onChange={(event) => changeWeight(piece, event.target.value)} aria-invalid={Boolean(weightErrors[piece.id])} /></label>
                  <label>Limit<input aria-label={`${piece.name} limit`} type="number" min="0" step="1" value={piece.limit ?? ""} onChange={(event) => updatePiece(piece.id, { limit: event.target.value === "" ? null : Math.max(0, Number(event.target.value)) })} /></label>
                </div>
                {weightErrors[piece.id] && <div role="alert" className="text-xs text-destructive">{weightErrors[piece.id]}</div>}
                <div className="mt-2 flex gap-1"><Button type="button" size="xs" variant="outline" onClick={() => rename(piece)}>Rename</Button><Button type="button" size="xs" variant="destructive" onClick={() => remove(piece)}>Delete</Button></div>
              </article>
            ))}
          </div>
          {!project.pieces.length && <p className="rounded border border-dashed p-4 text-muted-foreground">No pieces yet. Capture a selection from Map.</p>}
        </section>
        <aside className="space-y-3">
          {selected && <section className="rounded-lg border p-3" aria-label="Socket inspector">
            <h2 className="font-medium">Socket inspector — {selected.name}</h2>
            <p className="text-xs text-muted-foreground">Boundary strips use shared-cell order. Private internal sockets only match their sibling.</p>
            <div className="mt-2 space-y-1">{selectedEdges.map((edge) => {
              const count = legalNeighbourCount(project, selected, edge.side, edge.index);
              const override = selected.edge_overrides.find((item) => item.side === edge.side && item.index === edge.index);
              return <div key={`${edge.side}${edge.index}`} className={`rounded border p-1 ${count === 0 ? "border-destructive bg-destructive/10" : ""}`}>
                <div className="flex items-center justify-between gap-2"><span className="font-mono uppercase">{edge.side}{edge.index}</span><span className={count === 0 ? "font-bold text-destructive" : "text-muted-foreground"}>{count} legal neighbours{count === 0 ? " — ZERO" : ""}</span></div>
                <div className="text-xs">{edgeText(edge)}</div>
                <label className="mt-1 flex items-center gap-1 text-xs">Label override
                  <select aria-label={`Label ${edge.side}${edge.index}`} value={override?.label ?? ""} onChange={(event) => assignLabel(edge, event.target.value)}><option value="">Derived strip</option>{project.socket_labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select>
                </label>
              </div>;
            })}</div>
          </section>}
          <section className="rounded-lg border p-3" aria-label="Socket label vocabulary">
            <h2 className="font-medium">Socket label vocabulary</h2>
            <div className="mt-2 flex flex-wrap gap-1"><Input aria-label="New socket label" placeholder="label name" value={labelDraft} onChange={(event) => setLabelDraft(event.target.value)} /><select aria-label="New socket label kind" value={labelKind} onChange={(event) => setLabelKind(event.target.value as "symmetric" | "directed")}><option value="symmetric">symmetric</option><option value="directed">directed</option></select>{labelKind === "directed" && <select aria-label="Directed partner" value={partner} onChange={(event) => setPartner(event.target.value)}><option value="">partner</option>{project.socket_labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select>}<Button type="button" size="sm" onClick={createLabel}>Add</Button></div>
            <div className="mt-2 space-y-1">{project.socket_labels.map((label) => <div key={label.id} className="flex items-center gap-1"><Input aria-label={`Socket label ${label.id}`} value={label.name} onChange={(event) => updateLabel(label.id, { name: event.target.value })} /><span className="text-xs text-muted-foreground">{socketKindLabel(label.kind)}</span><Button type="button" size="xs" variant="ghost" onClick={() => deleteLabel(label.id)}>Delete label {label.name}</Button></div>)}</div>
          </section>
          <section className="rounded-lg border p-3" aria-label="Socket compatibility"><h2 className="font-medium">Recipe socket compatibility</h2>{project.recipes.map((recipe) => <div key={recipe.id} className="mt-2"><div>{recipe.name || `Recipe ${recipe.id}`}</div>{recipe.socket_compat.map(([left, right], index) => <div key={`${left}-${right}-${index}`} className="flex items-center gap-1 text-xs"><span>{left} ↔ {right}</span><Button type="button" size="xs" variant="ghost" aria-label={`Remove compatibility ${left} ${right}`} onClick={() => removeCompat(recipe.id, index)}>Remove</Button></div>)}<div className="flex gap-1"><select aria-label={`Compatibility left ${recipe.name}`} value={newCompat[0]} onChange={(event) => setNewCompat([event.target.value, newCompat[1]])}><option value="">left</option>{project.socket_labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select><select aria-label={`Compatibility right ${recipe.name}`} value={newCompat[1]} onChange={(event) => setNewCompat([newCompat[0], event.target.value])}><option value="">right</option>{project.socket_labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select><Button type="button" size="xs" onClick={() => addCompat(recipe.id)}>Add pair</Button></div></div>)}</section>
          <section aria-label="WFC validation" className="rounded-lg border p-3"><h2 className="font-medium">Validation</h2>{validation.length ? <ul className="list-disc pl-5 text-destructive">{validation.map((issue, index) => <li key={`${issue}-${index}`}>{issue}</li>)}</ul> : <p className="text-xs text-muted-foreground">No validation issues.</p>}</section>
        </aside>
      </div>
    </div>
  );
}

export default WfcTab;
