import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { GameMap, Hunk, WfcPiece, WfcProject, WfcRecipe } from "../types";
import { encode } from "../rle";
import { Button } from "./ui/button";
import { Input } from "./ui/input";

export interface RecipePanelProps {
  project: WfcProject;
  pieces: WfcPiece[];
  tiles: HTMLCanvasElement[];
  map: GameMap;
  onProjectChange: (project: WfcProject) => void;
  onMapChange?: (map: GameMap) => void;
  saveMap?: (map: GameMap) => Promise<void>;
}

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

export function RecipePanel({ project, pieces, tiles, map, onProjectChange, onMapChange, saveMap }: RecipePanelProps) {
  const [selectedId, setSelectedId] = useState<number | null>(() => project.recipes[0]?.id ?? null);
  const [selectedAnchorPiece, setSelectedAnchorPiece] = useState<number | null>(null);
  const [hoverSlot, setHoverSlot] = useState<SlotSize | null>(null);
  const [anchorError, setAnchorError] = useState<string | null>(null);
  const [generated, setGenerated] = useState<Hunk | null>(null);
  const [generationError, setGenerationError] = useState<string | null>(null);
  const [errorSlot, setErrorSlot] = useState<SlotSize | null>(null);

  const recipe = project.recipes.find((item) => item.id === selectedId) ?? project.recipes[0] ?? null;
  const symbols = recipe ? recipeSymbolCount(recipe, pieces, project.slot_w, project.slot_h) : 0;
  const memory = recipe ? recipeMemoryEstimate(recipe, pieces, project.slot_w, project.slot_h) : 0;
  const orphans = recipe ? orphanedAnchors(recipe, pieces, project.slot_w, project.slot_h) : [];
  const maxW = project.slot_w > 0 ? Math.floor(256 / project.slot_w) : 0;
  const maxH = project.slot_h > 0 ? Math.floor(256 / project.slot_h) : 0;

  useEffect(() => {
    if (selectedId !== null && !project.recipes.some((item) => item.id === selectedId)) {
      setSelectedId(project.recipes[0]?.id ?? null);
    }
  }, [project.recipes, selectedId]);

  const updateRecipe = (update: Partial<WfcRecipe>) => {
    if (!recipe) return;
    onProjectChange({ ...project, recipes: project.recipes.map((item) => item.id === recipe.id ? { ...item, ...update } : item) });
  };

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

  const removeAnchor = (index: number) => updateRecipe({ anchors: recipe?.anchors.filter((_, item) => item !== index) ?? [] });

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

  return <section aria-label="WFC recipes" className="col-span-full rounded-lg border p-3">
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
          <div className="grid gap-1 sm:grid-cols-2">{pieces.map((piece) => <label key={piece.id} className="flex items-center gap-2"><input type="checkbox" aria-label={`Recipe piece ${piece.name || piece.id}`} checked={recipe.piece_ids.includes(piece.id)} onChange={(event) => updateRecipe({ piece_ids: event.target.checked ? [...recipe.piece_ids, piece.id] : recipe.piece_ids.filter((id) => id !== piece.id) })} />{piece.name || `Piece ${piece.id}`}</label>)}</div>
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
        <fieldset className="rounded border p-2"><legend className="px-1 text-xs font-medium">Socket compatibility</legend>
          {recipe.socket_compat.length === 0 && <p className="text-xs text-muted-foreground">No extra compatible label pairs.</p>}
          {recipe.socket_compat.map(([left, right], index) => <div className="flex items-center gap-2 text-xs" key={`${left}-${right}-${index}`}><span>{left} ↔ {right}</span><Button type="button" size="xs" variant="ghost" onClick={() => updateRecipe({ socket_compat: recipe.socket_compat.filter((_, item) => item !== index) })}>Remove</Button></div>)}
          <CompatAdder labels={project.socket_labels.map((label) => ({ id: label.id, name: label.name }))} onAdd={(pair) => updateRecipe({ socket_compat: [...recipe.socket_compat, pair] })} />
        </fieldset>
      </div>
      <div className="space-y-3">
        <fieldset className="rounded border p-2"><legend className="px-1 text-xs font-medium">Anchors</legend>
          <div className="mb-2 flex flex-wrap gap-1">{pieces.map((piece) => <Button key={piece.id} type="button" size="xs" variant={selectedAnchorPiece === piece.id ? "default" : "outline"} onClick={() => setSelectedAnchorPiece(piece.id)}>Pick {piece.name || `piece ${piece.id}`}</Button>)}</div>
          {anchorError && <div role="alert" className="mb-2 text-destructive">{anchorError}</div>}
          <div className="grid max-h-72 max-w-full overflow-auto border bg-muted/20" style={{ gridTemplateColumns: `repeat(${Math.max(1, recipe.size_w)}, minmax(18px, 1fr))` }} onMouseLeave={() => setHoverSlot(null)}>
            {gridCells.map(({ sx, sy }) => { const isGhost = Boolean(hoverSlot && ghost && sx >= hoverSlot.w && sx < hoverSlot.w + ghost.w && sy >= hoverSlot.h && sy < hoverSlot.h + ghost.h); const isError = errorSlot?.w === sx && errorSlot?.h === sy; return <button key={`${sx}-${sy}`} type="button" aria-label={`Anchor slot ${sx},${sy}`} className={`aspect-square border text-[9px] ${occupied(sx, sy) ? "bg-primary/40" : ""} ${isGhost ? "bg-accent" : ""} ${isError ? "bg-destructive text-destructive-foreground" : ""}`} onMouseEnter={() => setHoverSlot({ w: sx, h: sy })} onFocus={() => setHoverSlot({ w: sx, h: sy })} onClick={() => addAnchor(sx, sy)}>{isError ? "!" : ""}</button>; })}
          </div>
          <p className="mt-1 text-xs text-muted-foreground">Pick a piece, then click its top-left slot. Ghost shows footprint.</p>
          <ul className="mt-2 space-y-1 text-xs">{recipe.anchors.map((anchor, index) => <li key={`${anchor.piece_id}-${anchor.sx}-${anchor.sy}-${index}`} className="flex items-center justify-between gap-2"><span>{pieces.find((piece) => piece.id === anchor.piece_id)?.name || `Piece ${anchor.piece_id}`} at ({anchor.sx}, {anchor.sy})</span><Button type="button" size="xs" variant="ghost" onClick={() => removeAnchor(index)}>Remove</Button></li>)}</ul>
        </fieldset>
        <div className="flex flex-wrap items-center gap-2"><Button type="button" onClick={() => runGenerate(recipe.seed)}>Generate</Button><Button type="button" variant="outline" onClick={() => runGenerate(recipe.seed + 1)}>Reroll (seed + 1)</Button>{generated && <Button type="button" variant="outline" onClick={saveStamp}>Save as stamp</Button>}</div>
        {generationError && <div role="alert" className="whitespace-pre-wrap rounded border border-destructive p-2 text-destructive">{generationError}{errorSlot && ` — failing slot (${errorSlot.w}, ${errorSlot.h}) highlighted.`}</div>}
        {generated && <div className="rounded border p-2"><div className="mb-2 text-xs text-muted-foreground">Generated {generated.w}×{generated.h} tiles · seed {generated.seed} · attempts {generated.attempts} · {generated.slots.length} slot symbols</div><HunkPreview hunk={generated} tiles={tiles} pieceIds={recipe.piece_ids} pieces={pieces} /></div>}
      </div>
    </div>}
  </section>;
}

function CompatAdder({ labels, onAdd }: { labels: { id: number; name: string }[]; onAdd: (pair: [number, number]) => void }) {
  const [left, setLeft] = useState("");
  const [right, setRight] = useState("");
  return <div className="mt-1 flex gap-1"><select aria-label="Add compatibility left" value={left} onChange={(event) => setLeft(event.target.value)}><option value="">left</option>{labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select><select aria-label="Add compatibility right" value={right} onChange={(event) => setRight(event.target.value)}><option value="">right</option>{labels.map((label) => <option key={label.id} value={label.id}>{label.name}</option>)}</select><Button type="button" size="xs" onClick={() => { if (left && right) { onAdd([Number(left), Number(right)]); setLeft(""); setRight(""); } }}>Add pair</Button></div>;
}

export default RecipePanel;
