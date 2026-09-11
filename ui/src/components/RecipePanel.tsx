import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { GameMap, Hunk, RuleSpec, SocketKind, WfcPiece, WfcProject, WfcRecipe } from "../types";
import { encode } from "../rle";
import { derivePieceSockets, legalNeighbourCount, nextWfcId, socketKindLabel, type SocketDisplay } from "../wfc";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export interface RecipePanelProps {
  project: WfcProject;
  pieces: WfcPiece[];
  tiles: HTMLCanvasElement[];
  map: GameMap;
  mode?: RecipePanelMode;
  validation?: string[];
  onProjectChange: (project: WfcProject) => void;
  onMapChange?: (map: GameMap) => void;
  saveMap?: (map: GameMap) => Promise<void>;
}

export type RecipePanelMode = "all" | "recipe" | "generate" | "advanced";

export interface SlotSize {
  w: number;
  h: number;
}

export function screenToSlots(slotW: number, slotH: number, screensW: number, screensH: number): SlotSize {
  return {
    w: Math.max(1, Math.floor((screensW * 8) / Math.max(1, slotW))),
    h: Math.max(1, Math.floor((screensH * 4) / Math.max(1, slotH))),
  };
}

export function recipeSymbolCount(recipe: WfcRecipe, pieces: WfcPiece[], slotW: number, slotH: number): number {
  return recipe.piece_ids.reduce((total, id) => {
    const piece = pieces.find((item) => item.id === id);
    if (!piece || slotW <= 0 || slotH <= 0) return total;
    return total + Math.floor(piece.tw / slotW) * Math.floor(piece.th / slotH);
  }, 0);
}

export function recipeMemoryEstimate(recipe: WfcRecipe, pieces: WfcPiece[], slotW: number, slotH: number): number {
  return recipe.size_w * recipe.size_h * recipeSymbolCount(recipe, pieces, slotW, slotH) * 4 * 2;
}

export function parseErrorSlot(message: string): SlotSize | null {
  const match = message.match(/(?:slot\s*)?\(\s*(\d+)\s*,\s*(\d+)\s*\)/i);
  return match ? { w: Number(match[1]), h: Number(match[2]) } : null;
}

export function orphanedAnchors(recipe: WfcRecipe, pieces: WfcPiece[], slotW: number, slotH: number): number[] {
  return recipe.anchors.flatMap((anchor, index) => {
    const piece = pieces.find((item) => item.id === anchor.piece_id);
    if (!piece || slotW <= 0 || slotH <= 0) return [index];
    const w = Math.ceil(piece.tw / slotW);
    const h = Math.ceil(piece.th / slotH);
    return anchor.sx + w > recipe.size_w || anchor.sy + h > recipe.size_h ? [index] : [];
  });
}

const PRESETS = [
  ["one", "1 screen", 1, 1],
  ["two", "2×2 screens", 2, 2],
  ["four", "4×4 screens", 4, 4],
  ["quarter", "Quarter map", 16, 32],
  ["full", "Full map", 32, 64],
] as const;

function presetSize(id: string, slotW: number, slotH: number): SlotSize | null {
  const preset = PRESETS.find(([key]) => key === id);
  return preset ? screenToSlots(slotW, slotH, preset[2], preset[3]) : null;
}

function sizePreset(recipe: WfcRecipe, slotW: number, slotH: number): string {
  return PRESETS.find(([, , screensW, screensH]) => {
    const size = screenToSlots(slotW, slotH, screensW, screensH);
    return size.w === recipe.size_w && size.h === recipe.size_h;
  })?.[0] ?? "free";
}

function pieceFootprint(piece: WfcPiece | undefined, slotW: number, slotH: number): SlotSize {
  return {
    w: piece && slotW > 0 ? Math.max(1, Math.ceil(piece.tw / slotW)) : 1,
    h: piece && slotH > 0 ? Math.max(1, Math.ceil(piece.th / slotH)) : 1,
  };
}

function symbolPiece(symbol: number, pieceIds: number[], pieces: WfcPiece[], slotW: number, slotH: number): number {
  let offset = 0;
  for (const id of pieceIds) {
    const piece = pieces.find((item) => item.id === id);
    const count = piece ? Math.ceil(piece.tw / slotW) * Math.ceil(piece.th / slotH) : 0;
    if (symbol < offset + count) return id;
    offset += count;
  }
  return -1;
}

const HISTORY_LIMIT = 6;

function pieceName(pieces: WfcPiece[], id: number): string {
  return pieces.find((piece) => piece.id === id)?.name || `piece ${id}`;
}

function edgeText(edge: SocketDisplay): string {
  if (edge.kind === "label") return `label ${edge.label}`;
  if (edge.kind === "private") return "private internal (siblings only)";
  return `derived strip [${edge.tiles.join(", ")}]`;
}

export function ruleSummary(rule: RuleSpec, pieces: WfcPiece[]): string {
  switch (rule.kind) {
    case "count": return `Count ${pieceName(pieces, rule.piece_id)}${rule.min !== null ? ` at least ${rule.min}` : ""}${rule.max !== null ? ` at most ${rule.max}` : ""}`;
    case "exclude": return `Exclude ${pieceName(pieces, rule.a)} and ${pieceName(pieces, rule.b)} within ${rule.radius} slot${rule.radius === 1 ? "" : "s"}`;
    case "border": return `Border: ${rule.piece_ids.map((id) => pieceName(pieces, id)).join(", ") || "no pieces"}`;
    case "band": return `Band ${pieceName(pieces, rule.piece_id)} on rows ${rule.from_row}–${rule.to_row}`;
  }
}

function ruleDefaults(kind: RuleSpec["kind"], pieces: WfcPiece[], recipe: WfcRecipe): RuleSpec {
  const first = pieces[0]?.id ?? 0;
  const second = pieces[1]?.id ?? first;
  if (kind === "count") return { kind, piece_id: first, min: null, max: null };
  if (kind === "exclude") return { kind, a: first, b: second, radius: 1 };
  if (kind === "border") return { kind, piece_ids: pieces.length > 0 ? [first] : [] };
  return { kind, piece_id: first, from_row: 0, to_row: Math.max(0, recipe.size_h - 1) };
}

function HunkPreview({ hunk, tiles, pieceIds, pieces }: { hunk: Hunk; tiles: HTMLCanvasElement[]; pieceIds: number[]; pieces: WfcPiece[] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const scale = Math.max(1, Math.min(8, Math.floor(640 / Math.max(hunk.w, hunk.h, 1))));
    canvas.width = Math.max(1, hunk.w * scale);
    canvas.height = Math.max(1, hunk.h * scale);
    context.imageSmoothingEnabled = false;
    context.fillStyle = "#222";
    context.fillRect(0, 0, canvas.width, canvas.height);
    hunk.tiles.forEach((tile, index) => {
      const source = tiles[tile];
      const x = (index % hunk.w) * scale;
      const y = Math.floor(index / hunk.w) * scale;
      if (source) context.drawImage(source, x, y, scale, scale);
      else {
        context.fillStyle = `hsl(${(tile * 47) % 360} 55% 45%)`;
        context.fillRect(x, y, scale, scale);
      }
    });
    context.strokeStyle = "rgba(255,255,255,.45)";
    context.lineWidth = 1;
    for (let x = 0; x <= hunk.w; x += hunk.slot_w) {
      context.beginPath(); context.moveTo(x * scale + 0.5, 0); context.lineTo(x * scale + 0.5, canvas.height); context.stroke();
    }
    for (let y = 0; y <= hunk.h; y += hunk.slot_h) {
      context.beginPath(); context.moveTo(0, y * scale + 0.5); context.lineTo(canvas.width, y * scale + 0.5); context.stroke();
    }
    const gridW = hunk.w / hunk.slot_w;
    const gridH = hunk.h / hunk.slot_h;
    context.strokeStyle = "rgba(255,220,80,.9)";
    context.lineWidth = 2;
    for (let index = 0; index < hunk.slots.length; index += 1) {
      const sx = index % gridW;
      const sy = Math.floor(index / gridW);
      const current = symbolPiece(hunk.slots[index], pieceIds, pieces, hunk.slot_w, hunk.slot_h);
      if (sx + 1 < gridW && symbolPiece(hunk.slots[index + 1], pieceIds, pieces, hunk.slot_w, hunk.slot_h) !== current) {
        context.beginPath(); context.moveTo((sx + 1) * hunk.slot_w * scale, sy * hunk.slot_h * scale); context.lineTo((sx + 1) * hunk.slot_w * scale, (sy + 1) * hunk.slot_h * scale); context.stroke();
      }
      if (sy + 1 < gridH && symbolPiece(hunk.slots[index + gridW], pieceIds, pieces, hunk.slot_w, hunk.slot_h) !== current) {
        context.beginPath(); context.moveTo(sx * hunk.slot_w * scale, (sy + 1) * hunk.slot_h * scale); context.lineTo((sx + 1) * hunk.slot_w * scale, (sy + 1) * hunk.slot_h * scale); context.stroke();
      }
    }
    context.fillStyle = "rgba(255,255,255,.8)";
    context.font = `${Math.max(8, Math.min(14, scale * 2))}px monospace`;
    hunk.slots.forEach((symbol, index) => {
      const x = (index % (hunk.w / hunk.slot_w)) * hunk.slot_w * scale + 2;
      const y = Math.floor(index / (hunk.w / hunk.slot_w)) * hunk.slot_h * scale + 12;
      context.fillText(String(symbol), x, y);
    });
  }, [hunk, tiles, pieceIds, pieces]);
  return <canvas ref={ref} aria-label="Generated WFC hunk preview" style={{ imageRendering: "pixelated", maxWidth: "100%", height: "auto" }} />;
}

export function RecipePanel({ project, pieces, tiles, map, mode = "all", validation = [], onProjectChange, onMapChange, saveMap }: RecipePanelProps) {
  const [selectedId, setSelectedId] = useState<number | null>(() => project.recipes[0]?.id ?? null);
  const [selectedAnchorPiece, setSelectedAnchorPiece] = useState<number | null>(null);
  const [selectedPieceId, setSelectedPieceId] = useState<number | null>(() => pieces[0]?.id ?? null);
  const [labelDraft, setLabelDraft] = useState("");
  const [labelKind, setLabelKind] = useState<"symmetric" | "directed">("symmetric");
  const [partner, setPartner] = useState("");
  const [hoverSlot, setHoverSlot] = useState<SlotSize | null>(null);
  const [anchorError, setAnchorError] = useState<string | null>(null);
  const [generated, setGenerated] = useState<Hunk | null>(null);
  const [history, setHistory] = useState<Hunk[]>([]);
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [errorSlot, setErrorSlot] = useState<SlotSize | null>(null);

  const recipe = project.recipes.find((item) => item.id === selectedId) ?? project.recipes[0] ?? null;
  const selectedPiece = pieces.find((piece) => piece.id === selectedPieceId) ?? pieces[0] ?? null;
  const selectedEdges = useMemo(() => selectedPiece ? derivePieceSockets(selectedPiece, project.slot_w, project.slot_h) : [], [selectedPiece, project.slot_w, project.slot_h]);
  const symbols = recipe ? recipeSymbolCount(recipe, pieces, project.slot_w, project.slot_h) : 0;
  const memory = recipe ? recipeMemoryEstimate(recipe, pieces, project.slot_w, project.slot_h) : 0;
  const orphans = recipe ? orphanedAnchors(recipe, pieces, project.slot_w, project.slot_h) : [];
  const maxW = project.slot_w > 0 ? Math.floor(256 / project.slot_w) : 0;
  const maxH = project.slot_h > 0 ? Math.floor(256 / project.slot_h) : 0;
  const rules = recipe?.rules ?? [];

  useEffect(() => {
    if (selectedId !== null && !project.recipes.some((item) => item.id === selectedId)) {
      setSelectedId(project.recipes[0]?.id ?? null);
    }
  }, [project.recipes, selectedId]);

  useEffect(() => {
    if (selectedPieceId !== null && !pieces.some((piece) => piece.id === selectedPieceId)) {
      setSelectedPieceId(pieces[0]?.id ?? null);
    }
  }, [pieces, selectedPieceId]);

  useEffect(() => {
    if (selectedAnchorPiece !== null && !pieces.some((piece) => piece.id === selectedAnchorPiece)) {
      setSelectedAnchorPiece(pieces[0]?.id ?? null);
    }
  }, [pieces, selectedAnchorPiece]);

  const updateRecipe = (update: Partial<WfcRecipe>) => {
    if (!recipe) return;
    onProjectChange({ ...project, recipes: project.recipes.map((item) => item.id === recipe.id ? { ...item, ...update } : item) });
  };

  const assignLabel = (edge: SocketDisplay, value: string) => {
    if (!selectedPiece) return;
    const overrides = selectedPiece.edge_overrides;
    const nextOverrides = overrides.filter((item) => !(item.side === edge.side && item.index === edge.index));
    if (value) nextOverrides.push({ side: edge.side, index: edge.index, label: Number(value) });
    onProjectChange({ ...project, pieces: pieces.map((piece) => piece.id === selectedPiece.id ? { ...piece, edge_overrides: nextOverrides } : piece) });
  };

  const createLabel = () => {
    const name = labelDraft.trim();
    if (!name || (labelKind === "directed" && !partner)) return;
    const id = nextWfcId(project.socket_labels);
    const kind: SocketKind = labelKind === "directed" ? { directed: { partner: Number(partner) } } : "symmetric";
    onProjectChange({ ...project, socket_labels: [...project.socket_labels, { id, name, kind }] });
    setLabelDraft("");
    setPartner("");
  };

  const updateLabel = (id: number, update: { name?: string; kind?: SocketKind }) => {
    onProjectChange({ ...project, socket_labels: project.socket_labels.map((label) => label.id === id ? { ...label, ...update } : label) });
  };

  const deleteLabel = (id: number) => {
    onProjectChange({
      ...project,
      socket_labels: project.socket_labels.filter((label) => label.id !== id),
      pieces: pieces.map((piece) => ({ ...piece, edge_overrides: piece.edge_overrides.filter((edge) => edge.label !== id) })),
      recipes: project.recipes.map((item) => ({ ...item, socket_compat: item.socket_compat.filter(([left, right]) => left !== id && right !== id) })),
    });
  };

  const addCompat = (pair: [number, number]) => updateRecipe({ socket_compat: [...(recipe?.socket_compat ?? []), pair] });
  const removeCompat = (index: number) => updateRecipe({ socket_compat: recipe?.socket_compat.filter((_, item) => item !== index) ?? [] });

  const createRecipe = () => {
    const id = project.recipes.reduce((max, item) => Math.max(max, item.id), 0) + 1;
    const size = screenToSlots(project.slot_w, project.slot_h, 1, 1);
    const next: WfcRecipe = {
      id,
      name: `Recipe ${id}`,
      piece_ids: pieces.map((piece) => piece.id),
      size_w: Math.min(size.w, maxW),
      size_h: Math.min(size.h, maxH),
      seed: 1,
      anchors: [],
      socket_compat: [],
      rules: [],
    };
    onProjectChange({ ...project, recipes: [...project.recipes, next] });
    setSelectedId(id);
  };

  const deleteRecipe = () => {
    if (!recipe) return;
    const next = project.recipes.filter((item) => item.id !== recipe.id);
    onProjectChange({ ...project, recipes: next });
    setSelectedId(next[0]?.id ?? null);
    setGenerated(null);
    setHistory([]);
  };

  const renameRecipe = () => {
    if (!recipe) return;
    const name = window.prompt("Recipe name", recipe.name)?.trim();
    if (name && name !== recipe.name) updateRecipe({ name });
  };

  const changeSize = (size: SlotSize) => {
    if (!recipe) return;
    const next = { ...recipe, size_w: Math.max(1, Math.min(maxW, size.w)), size_h: Math.max(1, Math.min(maxH, size.h)) };
    onProjectChange({ ...project, recipes: project.recipes.map((item) => item.id === recipe.id ? next : item) });
  };

  const addAnchor = (sx: number, sy: number) => {
    if (!recipe) return;
    const piece = pieces.find((item) => item.id === selectedAnchorPiece);
    if (!piece) {
      setAnchorError("Pick a piece before placing an anchor.");
      return;
    }
    if (!project.pieces.some((item) => item.id === piece.id) || !recipe.piece_ids.includes(piece.id)) {
      setAnchorError(`${piece.name || `piece ${piece.id}`} is not in this recipe's piece subset.`);
      return;
    }
    const footprint = pieceFootprint(piece, project.slot_w, project.slot_h);
    if (sx + footprint.w > recipe.size_w || sy + footprint.h > recipe.size_h) {
      setAnchorError(`Anchor for ${piece.name || `piece ${piece.id}`} at (${sx}, ${sy}) is out of bounds.`);
      return;
    }
    const overlap = recipe.anchors.some((anchor) => {
      const other = pieceFootprint(pieces.find((item) => item.id === anchor.piece_id), project.slot_w, project.slot_h);
      return sx < anchor.sx + other.w && sx + footprint.w > anchor.sx && sy < anchor.sy + other.h && sy + footprint.h > anchor.sy;
    });
    if (overlap) {
      setAnchorError(`Anchors overlap at slot (${sx}, ${sy}).`);
      return;
    }
    setAnchorError(null);
    updateRecipe({ anchors: [...recipe.anchors, { piece_id: piece.id, sx, sy }] });
  };

  const updateRule = (index: number, update: Partial<RuleSpec>) => {
    if (!recipe) return;
    const current = rules[index];
    if (!current) return;
    updateRecipe({ rules: rules.map((rule, item) => item === index ? { ...rule, ...update } as RuleSpec : rule) });
  };


  const removeAnchor = (index: number) => updateRecipe({ anchors: recipe?.anchors.filter((_, item) => item !== index) ?? [] });

  const clearAnchors = () => {
    if (!recipe || !recipe.anchors.length || !window.confirm(`Clear all anchors for ${recipe.name || `recipe ${recipe.id}`}?`)) return;
    updateRecipe({ anchors: [] });
    setSelectedAnchorPiece(null);
    setAnchorError(null);
  };

  const addRule = (kind: RuleSpec["kind"]) => updateRecipe({ rules: [...rules, ruleDefaults(kind, pieces, recipe!)] });
  const removeRule = (index: number) => updateRecipe({ rules: rules.filter((_, item) => item !== index) });


  const runGenerate = async (seed: number) => {
    if (!recipe) return;
    const nextRecipe = recipe.seed === seed ? recipe : { ...recipe, seed };
    if (nextRecipe !== recipe) onProjectChange({ ...project, recipes: project.recipes.map((item) => item.id === recipe.id ? nextRecipe : item) });
    setGenerationError(null);
    setErrorSlot(null);
    try {
      const hunk = await invoke<Hunk>("wfc_generate", {
        project: { ...project, recipes: project.recipes.map((item) => item.id === recipe.id ? nextRecipe : item) },
        recipeId: recipe.id,
        size: [nextRecipe.size_w, nextRecipe.size_h],
        seed,
      });
      setGenerated(hunk);
      setHistory((previous) => [...previous, hunk].slice(-HISTORY_LIMIT));
    } catch (error) {
      const message = String(error);
      setGenerationError(message);
      setErrorSlot(parseErrorSlot(message));
      setGenerated(null);
    }
  };

  const saveStamp = async () => {
    if (!recipe || !generated) return;
    const name = window.prompt("Stamp name", `${recipe.name || "WFC"} ${generated.seed}`)?.trim();
    if (!name) return;
    const stamps = map.stamps ?? [];
    const stamp = {
      id: stamps.reduce((max, item) => Math.max(max, item.id), -1) + 1,
      name,
      w: generated.w,
      h: generated.h,
      tiles: encode(generated.tiles),
      origin: { recipe_id: recipe.id, seed: generated.seed, slot_w: generated.slot_w, slot_h: generated.slot_h },
    };
    const nextMap = { ...map, stamps: [...stamps, stamp] };
    onMapChange?.(nextMap);
    await saveMap?.(nextMap);
  };

  const gridCells = useMemo(() => recipe ? Array.from({ length: recipe.size_w * recipe.size_h }, (_, index) => ({ sx: index % recipe.size_w, sy: Math.floor(index / recipe.size_w) })) : [], [recipe]);
  const occupied = (sx: number, sy: number) => recipe?.anchors.some((anchor) => {
    const footprint = pieceFootprint(pieces.find((piece) => piece.id === anchor.piece_id), project.slot_w, project.slot_h);
    return sx >= anchor.sx && sx < anchor.sx + footprint.w && sy >= anchor.sy && sy < anchor.sy + footprint.h;
  }) ?? false;
  const ghost = selectedAnchorPiece === null ? null : pieceFootprint(pieces.find((piece) => piece.id === selectedAnchorPiece), project.slot_w, project.slot_h);

  if (mode !== "all") {
    const title = mode === "recipe" ? "Recipe" : mode === "generate" ? "Generate" : "Advanced";
    return <section aria-label={`WFC ${title.toLowerCase()}`} data-testid={`wfc-${mode}-panel`} className="rounded-lg border p-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="font-medium">{title}</h2>
        {mode === "recipe" && <Button type="button" size="sm" onClick={createRecipe}>Create recipe</Button>}
        {!recipe && <span className="text-muted-foreground">{mode === "recipe" ? "Create a recipe to configure it." : "Create a recipe in the Recipe stage first."}</span>}
        {recipe && <select aria-label="Recipe" value={recipe.id} onChange={(event) => setSelectedId(Number(event.target.value))}>{project.recipes.map((item) => <option key={item.id} value={item.id}>{item.name || `Recipe ${item.id}`}</option>)}</select>}
        {mode === "recipe" && recipe && <><Button type="button" size="xs" variant="outline" onClick={renameRecipe}>Rename recipe</Button><Button type="button" size="xs" variant="destructive" onClick={deleteRecipe}>Delete recipe</Button></>}
      </div>
      {recipe && mode === "recipe" && <div className="mt-3 grid gap-3 lg:grid-cols-2">
        <label>Recipe name<Input aria-label="Recipe name" value={recipe.name} onChange={(event) => updateRecipe({ name: event.target.value })} /></label>
        <fieldset className="rounded border p-2"><legend className="px-1 text-xs font-medium">Piece subset</legend>
          <div className="mb-2 flex flex-wrap items-center gap-1"><Button type="button" size="xs" variant="outline" onClick={() => updateRecipe({ piece_ids: pieces.map((piece) => piece.id) })}>Use all pieces</Button><Button type="button" size="xs" variant="outline" onClick={() => updateRecipe({ piece_ids: [] })}>Clear pieces</Button><span className="text-xs text-muted-foreground">{recipe.piece_ids.length}/{pieces.length} selected</span></div>
          <div className="grid max-h-48 gap-1 overflow-auto sm:grid-cols-2">{pieces.map((piece) => <label key={piece.id} className="flex items-center gap-2"><input type="checkbox" aria-label={`Recipe piece ${piece.name || piece.id}`} checked={recipe.piece_ids.includes(piece.id)} onChange={(event) => updateRecipe({ piece_ids: event.target.checked ? [...recipe.piece_ids, piece.id] : recipe.piece_ids.filter((id) => id !== piece.id) })} />{piece.name || `Piece ${piece.id}`}</label>)}</div>
          <div className="mt-2 text-xs text-muted-foreground">{symbols} symbols · estimated solver memory {memory.toLocaleString()} bytes / 134,217,728</div>
        </fieldset>
        <fieldset className="rounded border p-2 lg:col-span-2"><legend className="px-1 text-xs font-medium">Hunk size</legend>
          <div className="flex flex-wrap items-center gap-2"><label>Preset<select aria-label="Hunk size preset" value={sizePreset(recipe, project.slot_w, project.slot_h)} onChange={(event) => { const size = presetSize(event.target.value, project.slot_w, project.slot_h); if (size) changeSize(size); }}>{PRESETS.map(([id, label]) => <option key={id} value={id}>{label}</option>)}<option value="free">Free slots</option></select></label><label>Slots W<Input aria-label="Recipe slot width" type="number" min="1" max={maxW} value={recipe.size_w} onChange={(event) => changeSize({ w: Number(event.target.value), h: recipe.size_h })} /></label><label>Slots H<Input aria-label="Recipe slot height" type="number" min="1" max={maxH} value={recipe.size_h} onChange={(event) => changeSize({ w: recipe.size_w, h: Number(event.target.value) })} /></label></div>
          <div className="mt-1 text-xs text-muted-foreground">{recipe.size_w}×{recipe.size_h} slots · {recipe.size_w * project.slot_w}×{recipe.size_h * project.slot_h} tiles</div>
          {orphans.length > 0 && <div role="alert" className="mt-2 text-destructive">Orphaned anchors after resize: {orphans.map((index) => `#${index + 1}`).join(", ")}. Resize or remove them; they were not dropped.</div>}
        </fieldset>
      </div>}
      {recipe && mode === "generate" && <div className="mt-3 space-y-3">
        <label className="block max-w-xs">Seed<Input aria-label="Recipe seed" type="number" min="0" step="1" value={recipe.seed} onChange={(event) => updateRecipe({ seed: Math.max(0, Number(event.target.value) || 0) })} /></label>
        <div className="flex flex-wrap items-center gap-2"><Button type="button" onClick={() => runGenerate(recipe.seed)}>Generate</Button><Button type="button" variant="outline" onClick={() => runGenerate(recipe.seed + 1)}>Reroll (seed + 1)</Button>{generated && <Button type="button" variant="outline" onClick={saveStamp}>Save as stamp</Button>}{history.length > 0 && <Button type="button" size="sm" variant="ghost" onClick={() => { setHistory([]); setGenerated(null); }}>Clear hunk history</Button>}</div>
        {generationError && <div role="alert" className="whitespace-pre-wrap rounded border border-destructive p-2 text-destructive">{generationError}{errorSlot && ` — failing slot (${errorSlot.w}, ${errorSlot.h}) highlighted.`}</div>}
        {history.length > 0 && <section aria-label="Hunk history" className="rounded border p-2"><div className="mb-1 flex items-center justify-between"><strong className="text-xs">Recent hunks</strong><span className="text-xs text-muted-foreground">{history.length}/{HISTORY_LIMIT}</span></div><div className="flex flex-wrap gap-1">{history.map((hunk, index) => <Button key={`${hunk.seed}-${index}`} type="button" size="xs" variant={generated === hunk ? "default" : "outline"} onClick={() => setGenerated(hunk)}>Seed {hunk.seed} · {hunk.attempts} attempts</Button>)}</div></section>}
        {generated && <div className="rounded border p-2"><div className="mb-2 text-xs text-muted-foreground">Generated {generated.w}×{generated.h} tiles · seed {generated.seed} · attempts {generated.attempts} · {generated.slots.length} slot symbols</div><HunkPreview hunk={generated} tiles={tiles} pieceIds={recipe.piece_ids} pieces={pieces} /></div>}
      </div>}
      {mode === "advanced" && <div className="mt-3 grid gap-3 lg:grid-cols-2">
        <div className="space-y-3">
          <section className="rounded-lg border p-3" aria-label="Socket inspector"><h3 className="font-medium">Socket inspector</h3><div className="mb-2 flex flex-wrap gap-1">{pieces.map((piece) => <Button key={piece.id} type="button" size="xs" variant={selectedPiece?.id === piece.id ? "default" : "outline"} onClick={() => setSelectedPieceId(piece.id)}>{piece.name || `Piece ${piece.id}`}</Button>)}</div>{selectedPiece && <div className="space-y-1">{selectedEdges.map((edge) => { const count = legalNeighbourCount(project, selectedPiece, edge.side, edge.index); const override = selectedPiece.edge_overrides.find((item) => item.side === edge.side && item.index === edge.index); return <div key={`${edge.side}${edge.index}`} className={`rounded border p-1 ${count === 0 ? "border-destructive bg-destructive/10" : ""}`}><div className="flex items-center justify-between gap-2"><span className="font-mono uppercase">{edge.side}{edge.index}</span><span className={count === 0 ? "font-bold text-destructive" : "text-muted-foreground"}>{count} legal neighbours{count === 0 ? " — ZERO" : ""}</span></div><div className="text-xs">{edgeText(edge)}</div><label className="mt-1 flex items-center gap-1 text-xs">Label override<select aria-label={`Label ${edge.side}${edge.index}`} value={override?.label ?? ""} onChange={(event) => assignLabel(edge, event.target.value)}><option value="">Derived strip</option>{project.socket_labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select></label></div>; })}</div>}</section>
          <section className="rounded-lg border p-3" aria-label="Socket label vocabulary"><h3 className="font-medium">Socket label vocabulary</h3><div className="mt-2 flex flex-wrap gap-1"><Input aria-label="New socket label" placeholder="label name" value={labelDraft} onChange={(event) => setLabelDraft(event.target.value)} /><select aria-label="New socket label kind" value={labelKind} onChange={(event) => setLabelKind(event.target.value as "symmetric" | "directed")}><option value="symmetric">symmetric</option><option value="directed">directed</option></select>{labelKind === "directed" && <select aria-label="Directed partner" value={partner} onChange={(event) => setPartner(event.target.value)}><option value="">partner</option>{project.socket_labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select>}<Button type="button" size="sm" onClick={createLabel}>Add</Button></div><div className="mt-2 space-y-1">{project.socket_labels.map((label) => <div key={label.id} className="flex items-center gap-1"><Input aria-label={`Socket label ${label.id}`} value={label.name} onChange={(event) => updateLabel(label.id, { name: event.target.value })} /><span className="text-xs text-muted-foreground">{socketKindLabel(label.kind)}</span><Button type="button" size="xs" variant="ghost" onClick={() => deleteLabel(label.id)}>Delete label {label.name}</Button></div>)}</div></section>
          {recipe && <section className="rounded-lg border p-3" aria-label="Socket compatibility"><h3 className="font-medium">Recipe socket compatibility</h3>{recipe.socket_compat.length === 0 && <p className="text-xs text-muted-foreground">No extra compatible label pairs.</p>}{recipe.socket_compat.map(([left, right], index) => <div key={`${left}-${right}-${index}`} className="flex items-center gap-2 text-xs"><span>{left} ↔ {right}</span><Button type="button" size="xs" variant="ghost" aria-label={`Remove compatibility ${left} ${right}`} onClick={() => removeCompat(index)}>Remove</Button></div>)}<CompatAdder labels={project.socket_labels.map((label) => ({ id: label.id, name: label.name }))} onAdd={addCompat} /></section>}
        </div>
        {recipe && <div className="space-y-3">
          <details open className="rounded border p-2"><summary className="cursor-pointer font-medium">Anchors — fixed pieces for generation</summary><fieldset className="mt-2"><legend className="sr-only">Anchors</legend><div className="mb-2 flex flex-wrap gap-1">{pieces.map((piece) => <Button key={piece.id} type="button" size="xs" variant={selectedAnchorPiece === piece.id ? "default" : "outline"} onClick={() => setSelectedAnchorPiece(piece.id)}>Pick {piece.name || `piece ${piece.id}`}</Button>)}<Button type="button" size="xs" variant="destructive" disabled={!recipe.anchors.length} onClick={clearAnchors}>Clear all anchors</Button></div>{anchorError && <div role="alert" className="mb-2 text-destructive">{anchorError}</div>}<div className="grid max-h-72 max-w-full overflow-auto border bg-muted/20" style={{ gridTemplateColumns: `repeat(${Math.max(1, recipe.size_w)}, minmax(18px, 1fr))` }} onMouseLeave={() => setHoverSlot(null)}>{gridCells.map(({ sx, sy }) => { const isGhost = Boolean(hoverSlot && ghost && sx >= hoverSlot.w && sx < hoverSlot.w + ghost.w && sy >= hoverSlot.h && sy < hoverSlot.h + ghost.h); return <button key={`${sx}-${sy}`} type="button" aria-label={`Anchor slot ${sx},${sy}`} className={`aspect-square border text-[9px] ${occupied(sx, sy) ? "bg-primary/40" : ""} ${isGhost ? "bg-accent" : ""}`} onMouseEnter={() => setHoverSlot({ w: sx, h: sy })} onFocus={() => setHoverSlot({ w: sx, h: sy })} onClick={() => addAnchor(sx, sy)}>{""}</button>; })}</div><p className="mt-1 text-xs text-muted-foreground">Pick a piece, then click its top-left slot. Ghost shows footprint.</p><ul className="mt-2 space-y-1 text-xs">{recipe.anchors.map((anchor, index) => <li key={`${anchor.piece_id}-${anchor.sx}-${anchor.sy}-${index}`} className="flex items-center justify-between gap-2"><span>{pieces.find((piece) => piece.id === anchor.piece_id)?.name || `Piece ${anchor.piece_id}`} at ({anchor.sx}, {anchor.sy})</span><Button type="button" size="xs" variant="ghost" onClick={() => removeAnchor(index)}>Remove</Button></li>)}</ul></fieldset></details>
          <details open className="rounded border p-2"><summary className="cursor-pointer font-medium">Placement rules — optional constraints</summary><fieldset aria-label="Placement rules" className="mt-2"><legend className="sr-only">Placement rules</legend><div className="mb-2 flex flex-wrap gap-1"><Button type="button" size="xs" variant="outline" onClick={() => addRule("count")}>Add count rule</Button><Button type="button" size="xs" variant="outline" onClick={() => addRule("exclude")}>Add exclude rule</Button><Button type="button" size="xs" variant="outline" onClick={() => addRule("border")}>Add border rule</Button><Button type="button" size="xs" variant="outline" onClick={() => addRule("band")}>Add band rule</Button></div>{rules.length === 0 && <p className="text-xs text-muted-foreground">No placement rules.</p>}<div className="space-y-2">{rules.map((rule, index) => <div key={`${rule.kind}-${index}`} className="rounded border p-2 text-xs"><div className="mb-1 flex items-center justify-between gap-2"><span>{ruleSummary(rule, pieces)}</span><Button type="button" size="xs" variant="ghost" onClick={() => removeRule(index)}>Remove rule</Button></div>{rule.kind === "count" && <div className="flex flex-wrap gap-1"><select aria-label={`Count rule ${index} piece`} value={rule.piece_id} onChange={(event) => updateRule(index, { piece_id: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><Input aria-label={`Count rule ${index} minimum`} type="number" min="0" placeholder="min" value={rule.min ?? ""} onChange={(event) => updateRule(index, { min: event.target.value === "" ? null : Math.max(0, Number(event.target.value)) })} /><Input aria-label={`Count rule ${index} maximum`} type="number" min="0" placeholder="max" value={rule.max ?? ""} onChange={(event) => updateRule(index, { max: event.target.value === "" ? null : Math.max(0, Number(event.target.value)) })} /></div>}{rule.kind === "exclude" && <div className="flex flex-wrap gap-1"><select aria-label={`Exclude rule ${index} piece a`} value={rule.a} onChange={(event) => updateRule(index, { a: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><select aria-label={`Exclude rule ${index} piece b`} value={rule.b} onChange={(event) => updateRule(index, { b: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><Input aria-label={`Exclude rule ${index} radius`} type="number" min="1" value={rule.radius} onChange={(event) => updateRule(index, { radius: Math.max(1, Number(event.target.value) || 1) })} /></div>}{rule.kind === "border" && <div className="flex flex-wrap gap-1">{pieces.map((piece) => <label key={piece.id} className="flex items-center gap-1"><input type="checkbox" aria-label={`Border rule ${index} ${piece.name || piece.id}`} checked={rule.piece_ids.includes(piece.id)} onChange={(event) => updateRule(index, { piece_ids: event.target.checked ? [...rule.piece_ids, piece.id] : rule.piece_ids.filter((id) => id !== piece.id) })} />{piece.name || piece.id}</label>)}</div>}{rule.kind === "band" && <div className="flex flex-wrap gap-1"><select aria-label={`Band rule ${index} piece`} value={rule.piece_id} onChange={(event) => updateRule(index, { piece_id: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><Input aria-label={`Band rule ${index} from row`} type="number" min="0" max={Math.max(0, recipe.size_h - 1)} value={rule.from_row} onChange={(event) => updateRule(index, { from_row: Math.max(0, Number(event.target.value) || 0) })} /><Input aria-label={`Band rule ${index} to row`} type="number" min="0" max={Math.max(0, recipe.size_h - 1)} value={rule.to_row} onChange={(event) => updateRule(index, { to_row: Math.max(0, Number(event.target.value) || 0) })} /></div>}</div>)}</div></fieldset></details>
          <section aria-label="WFC validation" className="rounded-lg border p-3"><h3 className="font-medium">Validation</h3>{validation.length ? <ul className="list-disc pl-5 text-destructive">{validation.map((issue, index) => <li key={`${issue}-${index}`}>{issue}</li>)}</ul> : <p className="text-xs text-muted-foreground">No validation issues.</p>}</section>
        </div>}
      </div>}
    </section>;
  }

  return (<section aria-label="WFC recipes" className="col-span-full rounded-lg border p-3">
    <div className="flex flex-wrap items-center gap-2">
      <h2 className="font-medium">Recipes</h2>
      <Button type="button" size="sm" onClick={createRecipe}>Create recipe</Button>
      {!recipe && <span className="text-muted-foreground">Create a recipe to generate a hunk.</span>}
      {recipe && <>
        <select aria-label="Recipe" value={recipe.id} onChange={(event) => setSelectedId(Number(event.target.value))}>{project.recipes.map((item) => <option key={item.id} value={item.id}>{item.name || `Recipe ${item.id}`}</option>)}</select>
        <Button type="button" size="xs" variant="outline" onClick={renameRecipe}>Rename recipe</Button>
        <Button type="button" size="xs" variant="destructive" onClick={deleteRecipe}>Delete recipe</Button>
      </>}
    </div>
    {recipe && <div className="mt-3 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(280px,1fr)]">
      <div className="space-y-3">
        <div className="grid gap-2 sm:grid-cols-2">
          <label>Recipe name<Input aria-label="Recipe name" value={recipe.name} onChange={(event) => updateRecipe({ name: event.target.value })} /></label>
          <label>Seed<Input aria-label="Recipe seed" type="number" min="0" step="1" value={recipe.seed} onChange={(event) => updateRecipe({ seed: Math.max(0, Number(event.target.value) || 0) })} /></label>
        </div>
        <fieldset className="rounded border p-2"><legend className="px-1 text-xs font-medium">Piece subset</legend>
          <div className="mb-2 flex flex-wrap items-center gap-1">
            <Button type="button" size="xs" variant="outline" onClick={() => updateRecipe({ piece_ids: pieces.map((piece) => piece.id) })}>Use all pieces</Button>
            <Button type="button" size="xs" variant="outline" onClick={() => updateRecipe({ piece_ids: [] })}>Clear pieces</Button>
            <span className="text-xs text-muted-foreground">{recipe.piece_ids.length}/{pieces.length} selected</span>
          </div>
          <div className="grid max-h-48 gap-1 overflow-auto sm:grid-cols-2">{pieces.map((piece) => <label key={piece.id} className="flex items-center gap-2"><input type="checkbox" aria-label={`Recipe piece ${piece.name || piece.id}`} checked={recipe.piece_ids.includes(piece.id)} onChange={(event) => updateRecipe({ piece_ids: event.target.checked ? [...recipe.piece_ids, piece.id] : recipe.piece_ids.filter((id) => id !== piece.id) })} />{piece.name || `Piece ${piece.id}`}</label>)}</div>
          <div className="mt-2 text-xs text-muted-foreground">{symbols} symbols · estimated solver memory {memory.toLocaleString()} bytes / 134,217,728</div>
        </fieldset>
        <fieldset className="rounded border p-2"><legend className="px-1 text-xs font-medium">Hunk size</legend>
          <div className="flex flex-wrap items-center gap-2"><label>Preset<select aria-label="Hunk size preset" value={sizePreset(recipe, project.slot_w, project.slot_h)} onChange={(event) => { const size = presetSize(event.target.value, project.slot_w, project.slot_h); if (size) changeSize(size); }}>{PRESETS.map(([id, label]) => <option key={id} value={id}>{label}</option>)}<option value="free">Free slots</option></select></label>
            <label>Slots W<Input aria-label="Recipe slot width" type="number" min="1" max={maxW} value={recipe.size_w} onChange={(event) => changeSize({ w: Number(event.target.value), h: recipe.size_h })} /></label>
            <label>Slots H<Input aria-label="Recipe slot height" type="number" min="1" max={maxH} value={recipe.size_h} onChange={(event) => changeSize({ w: recipe.size_w, h: Number(event.target.value) })} /></label>
          </div>
          <div className="mt-1 text-xs text-muted-foreground">{recipe.size_w}×{recipe.size_h} slots · {recipe.size_w * project.slot_w}×{recipe.size_h * project.slot_h} tiles</div>
          {orphans.length > 0 && <div role="alert" className="mt-2 text-destructive">Orphaned anchors after resize: {orphans.map((index) => `#${index + 1}`).join(", ")}. Resize or remove them; they were not dropped.</div>}
        </fieldset>
        <details open className="rounded border p-2"><summary className="cursor-pointer font-medium">Placement rules — optional constraints</summary>
        <fieldset aria-label="Placement rules" className="mt-2"><legend className="sr-only">Placement rules</legend>
          <div className="mb-2 flex flex-wrap gap-1"><Button type="button" size="xs" variant="outline" onClick={() => addRule("count")}>Add count rule</Button><Button type="button" size="xs" variant="outline" onClick={() => addRule("exclude")}>Add exclude rule</Button><Button type="button" size="xs" variant="outline" onClick={() => addRule("border")}>Add border rule</Button><Button type="button" size="xs" variant="outline" onClick={() => addRule("band")}>Add band rule</Button></div>
          {rules.length === 0 && <p className="text-xs text-muted-foreground">No placement rules.</p>}
          <div className="space-y-2">{rules.map((rule, index) => <div key={`${rule.kind}-${index}`} className="rounded border p-2 text-xs">
            <div className="mb-1 flex items-center justify-between gap-2"><span>{ruleSummary(rule, pieces)}</span><Button type="button" size="xs" variant="ghost" onClick={() => removeRule(index)}>Remove rule</Button></div>
            {rule.kind === "count" && <div className="flex flex-wrap gap-1"><select aria-label={`Count rule ${index} piece`} value={rule.piece_id} onChange={(event) => updateRule(index, { piece_id: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><Input aria-label={`Count rule ${index} minimum`} type="number" min="0" placeholder="min" value={rule.min ?? ""} onChange={(event) => updateRule(index, { min: event.target.value === "" ? null : Math.max(0, Number(event.target.value)) })} /><Input aria-label={`Count rule ${index} maximum`} type="number" min="0" placeholder="max" value={rule.max ?? ""} onChange={(event) => updateRule(index, { max: event.target.value === "" ? null : Math.max(0, Number(event.target.value)) })} /></div>}
            {rule.kind === "exclude" && <div className="flex flex-wrap gap-1"><select aria-label={`Exclude rule ${index} piece a`} value={rule.a} onChange={(event) => updateRule(index, { a: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><select aria-label={`Exclude rule ${index} piece b`} value={rule.b} onChange={(event) => updateRule(index, { b: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><Input aria-label={`Exclude rule ${index} radius`} type="number" min="1" value={rule.radius} onChange={(event) => updateRule(index, { radius: Math.max(1, Number(event.target.value) || 1) })} /></div>}
            {rule.kind === "border" && <div className="flex flex-wrap gap-1">{pieces.map((piece) => <label key={piece.id} className="flex items-center gap-1"><input type="checkbox" aria-label={`Border rule ${index} ${piece.name || piece.id}`} checked={rule.piece_ids.includes(piece.id)} onChange={(event) => updateRule(index, { piece_ids: event.target.checked ? [...rule.piece_ids, piece.id] : rule.piece_ids.filter((id) => id !== piece.id) })} />{piece.name || piece.id}</label>)}</div>}
            {rule.kind === "band" && <div className="flex flex-wrap gap-1"><select aria-label={`Band rule ${index} piece`} value={rule.piece_id} onChange={(event) => updateRule(index, { piece_id: Number(event.target.value) })}>{pieces.map((piece) => <option key={piece.id} value={piece.id}>{piece.name || piece.id}</option>)}</select><Input aria-label={`Band rule ${index} from row`} type="number" min="0" max={Math.max(0, recipe.size_h - 1)} value={rule.from_row} onChange={(event) => updateRule(index, { from_row: Math.max(0, Number(event.target.value) || 0) })} /><Input aria-label={`Band rule ${index} to row`} type="number" min="0" max={Math.max(0, recipe.size_h - 1)} value={rule.to_row} onChange={(event) => updateRule(index, { to_row: Math.max(0, Number(event.target.value) || 0) })} /></div>}
          </div>)}</div>
        </fieldset></details>
      </div>
      <div className="space-y-3">
        <details open className="rounded border p-2"><summary className="cursor-pointer font-medium">Anchors — fixed pieces for generation</summary>
        <fieldset className="mt-2"><legend className="sr-only">Anchors</legend>
          <div className="mb-2 flex flex-wrap gap-1">{pieces.map((piece) => <Button key={piece.id} type="button" size="xs" variant={selectedAnchorPiece === piece.id ? "default" : "outline"} onClick={() => setSelectedAnchorPiece(piece.id)}>Pick {piece.name || `piece ${piece.id}`}</Button>)}</div>
          {anchorError && <div role="alert" className="mb-2 text-destructive">{anchorError}</div>}
          <div className="grid max-h-72 max-w-full overflow-auto border bg-muted/20" style={{ gridTemplateColumns: `repeat(${Math.max(1, recipe.size_w)}, minmax(18px, 1fr))` }} onMouseLeave={() => setHoverSlot(null)}>
            {gridCells.map(({ sx, sy }) => { const isGhost = Boolean(hoverSlot && ghost && sx >= hoverSlot.w && sx < hoverSlot.w + ghost.w && sy >= hoverSlot.h && sy < hoverSlot.h + ghost.h); const isError = errorSlot?.w === sx && errorSlot?.h === sy; return <button key={`${sx}-${sy}`} type="button" aria-label={`Anchor slot ${sx},${sy}`} className={`aspect-square border text-[9px] ${occupied(sx, sy) ? "bg-primary/40" : ""} ${isGhost ? "bg-accent" : ""} ${isError ? "bg-destructive text-destructive-foreground" : ""}`} onMouseEnter={() => setHoverSlot({ w: sx, h: sy })} onFocus={() => setHoverSlot({ w: sx, h: sy })} onClick={() => addAnchor(sx, sy)}>{isError ? "!" : ""}</button>; })}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Pick a piece, then click its top-left slot. Ghost shows footprint.</p>
          <ul className="mt-2 space-y-1 text-xs">{recipe.anchors.map((anchor, index) => <li key={`${anchor.piece_id}-${anchor.sx}-${anchor.sy}-${index}`} className="flex items-center justify-between gap-2"><span>{pieces.find((piece) => piece.id === anchor.piece_id)?.name || `Piece ${anchor.piece_id}`} at ({anchor.sx}, {anchor.sy})</span><Button type="button" size="xs" variant="ghost" onClick={() => removeAnchor(index)}>Remove</Button></li>)}</ul>
        </fieldset></details>
        <div className="flex flex-wrap items-center gap-2"><Button type="button" onClick={() => runGenerate(recipe.seed)}>Generate</Button><Button type="button" variant="outline" onClick={() => runGenerate(recipe.seed + 1)}>Reroll (seed + 1)</Button>{generated && <Button type="button" variant="outline" onClick={saveStamp}>Save as stamp</Button>}{history.length > 0 && <Button type="button" size="sm" variant="ghost" onClick={() => { setHistory([]); setGenerated(null); }}>Clear hunk history</Button>}</div>
        {generationError && <div role="alert" className="whitespace-pre-wrap rounded border border-destructive p-2 text-destructive">{generationError}{errorSlot && ` — failing slot (${errorSlot.w}, ${errorSlot.h}) highlighted.`}</div>}
        {history.length > 0 && <section aria-label="Hunk history" className="rounded border p-2"><div className="mb-1 flex items-center justify-between"><strong className="text-xs">Recent hunks</strong><span className="text-xs text-muted-foreground">{history.length}/{HISTORY_LIMIT}</span></div><div className="flex flex-wrap gap-1">{history.map((hunk, index) => <Button key={`${hunk.seed}-${index}`} type="button" size="xs" variant={generated === hunk ? "default" : "outline"} onClick={() => setGenerated(hunk)}>Seed {hunk.seed} · {hunk.attempts} attempts</Button>)}</div></section>}
        {generated && <div className="rounded border p-2"><div className="mb-2 text-xs text-muted-foreground">Generated {generated.w}×{generated.h} tiles · seed {generated.seed} · attempts {generated.attempts} · {generated.slots.length} slot symbols</div><HunkPreview hunk={generated} tiles={tiles} pieceIds={recipe.piece_ids} pieces={pieces} /></div>}
      </div>
    </div>}
  </section>);
}

function CompatAdder({ labels, onAdd }: { labels: { id: number; name: string }[]; onAdd: (pair: [number, number]) => void }) {
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  return <div className="mt-1 flex gap-1"><select aria-label="Add compatibility left" value={left} onChange={(event) => setLeft(event.target.value)}><option value="">left</option>{labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select><select aria-label="Add compatibility right" value={right} onChange={(event) => setRight(event.target.value)}><option value="">right</option>{labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select><Button type="button" size="xs" onClick={() => { if (left && right) { onAdd([Number(left), Number(right)]); setLeft(""); setRight(""); } }}>Add pair</Button></div>;
}

export default RecipePanel;
