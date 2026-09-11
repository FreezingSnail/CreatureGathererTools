import type { EdgeSocket, RuleSpec, Side, SocketLabel, SocketKind, WfcPiece, WfcProject } from "./types";
import { decode } from "./rle";

export const WFC_SIDES: readonly Side[] = ["n", "e", "s", "w"];

export interface EdgeStrip {
  side: Side;
  index: number;
  tiles: number[];
}

export type SocketDisplayKind = "derived" | "private" | "label";

export interface SocketDisplay extends EdgeStrip {
  kind: SocketDisplayKind;
  label?: number;
  privateId?: string;
}

export function pieceTiles(piece: Pick<WfcPiece, "tiles" | "tw" | "th">): number[] {
  try {
    return decode(piece.tiles, piece.tw * piece.th);
  } catch {
    return [];
  }
}

/** Extract a row-major tile rectangle from a piece, clipping to its bounds. */
export function extractThumbnailTiles(
  piece: Pick<WfcPiece, "tiles" | "tw" | "th">,
  x = 0,
  y = 0,
  width = piece.tw,
  height = piece.th,
): number[] {
  const tiles = pieceTiles(piece);
  const startX = Math.max(0, Math.floor(x));
  const startY = Math.max(0, Math.floor(y));
  const endX = Math.min(piece.tw, startX + Math.max(0, Math.floor(width)));
  const endY = Math.min(piece.th, startY + Math.max(0, Math.floor(height)));
  const result: number[] = [];
  for (let row = startY; row < endY; row += 1) {
    result.push(...tiles.slice(row * piece.tw + startX, row * piece.tw + endX));
  }
  return result;
}

/** Boundary tile strips for each slot along each side, in shared-cell order. */
export function deriveEdgeStrips(
  piece: Pick<WfcPiece, "tiles" | "tw" | "th">,
  slotW: number,
  slotH: number,
): Record<Side, EdgeStrip[]> {
  const tiles = pieceTiles(piece);
  const cols = slotW > 0 ? Math.floor(piece.tw / slotW) : 0;
  const rows = slotH > 0 ? Math.floor(piece.th / slotH) : 0;
  const result: Record<Side, EdgeStrip[]> = { n: [], e: [], s: [], w: [] };
  for (let index = 0; index < cols; index += 1) {
    const x = index * slotW;
    result.n.push({ side: "n", index, tiles: tiles.slice(x, x + slotW) });
    const bottom = Math.max(0, piece.th - 1) * piece.tw + x;
    result.s.push({ side: "s", index, tiles: tiles.slice(bottom, bottom + slotW) });
  }
  for (let index = 0; index < rows; index += 1) {
    const y = index * slotH;
    const east: number[] = [];
    const west: number[] = [];
    for (let row = 0; row < slotH; row += 1) {
      east.push(tiles[(y + row) * piece.tw + piece.tw - 1] ?? 0);
      west.push(tiles[(y + row) * piece.tw] ?? 0);
    }
    result.e.push({ side: "e", index, tiles: east });
    result.w.push({ side: "w", index, tiles: west });
  }
  return result;
}

function edgeOverride(piece: WfcPiece, side: Side, index: number): EdgeSocket | undefined {
  return (piece.edge_overrides ?? []).find((edge) => edge.side === side && edge.index === index);
}

function privateId(piece: WfcPiece, side: Side, index: number, slotW: number, slotH: number): string | undefined {
  const cols = slotW > 0 ? Math.floor(piece.tw / slotW) : 0;
  const rows = slotH > 0 ? Math.floor(piece.th / slotH) : 0;
  const col = side === "n" || side === "s" ? index : side === "e" ? cols - 1 : 0;
  const row = side === "e" || side === "w" ? index : side === "s" ? rows - 1 : 0;
  const internal =
    (side === "n" && row > 0) ||
    (side === "e" && col + 1 < cols) ||
    (side === "s" && row + 1 < rows) ||
    (side === "w" && col > 0);
  if (!internal) return undefined;
  const ownerSide: Side = side === "n" ? "s" : side === "w" ? "e" : side;
  return `${piece.id}:${col}:${row}:${ownerSide}`;
}

/** Resolve the four-sided socket display for every slot cell in a piece. */
export function derivePieceSockets(piece: WfcPiece, slotW: number, slotH: number): SocketDisplay[] {
  const strips = deriveEdgeStrips(piece, slotW, slotH);
  const result: SocketDisplay[] = [];
  for (const side of WFC_SIDES) {
    for (const strip of strips[side]) {
      const override = edgeOverride(piece, side, strip.index);
      const internal = privateId(piece, side, strip.index, slotW, slotH);
      result.push(override
        ? { ...strip, kind: "label", label: override.label }
        : internal
          ? { ...strip, kind: "private", privateId: internal }
          : { ...strip, kind: "derived" });
    }
  }
  return result;
}

function labelsMatch(left: number, right: number, labels: SocketLabel[], compat: [number, number][]): boolean {
  if (compat.some(([a, b]) => (a === left && b === right) || (a === right && b === left))) return true;
  const leftKind = labels.find((label) => label.id === left)?.kind;
  const rightKind = labels.find((label) => label.id === right)?.kind;
  if (leftKind === "symmetric" && rightKind === "symmetric") return left === right;
  if (typeof leftKind === "object" && leftKind.directed.partner === right) return true;
  if (typeof rightKind === "object" && rightKind.directed.partner === left) return true;
  return false;
}

function socketsMatch(a: SocketDisplay, b: SocketDisplay, project: WfcProject): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "derived") return a.tiles.join(",") === b.tiles.join(",");
  if (a.kind === "private") return a.privateId === b.privateId;
  return a.label !== undefined && b.label !== undefined
    ? labelsMatch(a.label, b.label, project.socket_labels ?? [], project.recipes.flatMap((recipe) => recipe.socket_compat ?? []))
    : false;
}

/** Count pieces with at least one compatible facing boundary edge. */
export function legalNeighbourCount(
  project: WfcProject,
  piece: WfcPiece,
  side: Side,
  index: number,
): number {
  const source = derivePieceSockets(piece, project.slot_w, project.slot_h)
    .find((edge) => edge.side === side && edge.index === index);
  if (!source) return 0;
  const opposite: Record<Side, Side> = { n: "s", e: "w", s: "n", w: "e" };
  let count = 0;
  for (const candidate of project.pieces) {
    if (candidate.id === piece.id) continue;
    const edges = derivePieceSockets(candidate, project.slot_w, project.slot_h);
    if (edges.some((edge) => edge.side === opposite[side] && socketsMatch(source, edge, project))) count += 1;
  }
  return count;
}

export function legalNeighbourCounts(project: WfcProject, piece: WfcPiece): Record<Side, number[]> {
  const edges = derivePieceSockets(piece, project.slot_w, project.slot_h);
  return {
    n: edges.filter((edge) => edge.side === "n").map((edge) => legalNeighbourCount(project, piece, "n", edge.index)),
    e: edges.filter((edge) => edge.side === "e").map((edge) => legalNeighbourCount(project, piece, "e", edge.index)),
    s: edges.filter((edge) => edge.side === "s").map((edge) => legalNeighbourCount(project, piece, "s", edge.index)),
    w: edges.filter((edge) => edge.side === "w").map((edge) => legalNeighbourCount(project, piece, "w", edge.index)),
  };
}

export function nextWfcId(values: { id: number }[]): number {
  return values.reduce((max, value) => Math.max(max, value.id), 0) + 1;
}

/** Remove pieces and every recipe reference that would otherwise dangle. */
export function removeWfcPieces(project: WfcProject, removedIds: readonly number[]): WfcProject {
  const removed = new Set(removedIds);
  if (!removed.size) return project;
  return {
    ...project,
    pieces: project.pieces.filter((piece) => !removed.has(piece.id)),
    recipes: project.recipes.map((recipe) => ({
      ...recipe,
      piece_ids: recipe.piece_ids.filter((id) => !removed.has(id)),
      anchors: recipe.anchors.filter((anchor) => !removed.has(anchor.piece_id)),
      rules: recipe.rules?.flatMap((rule): RuleSpec[] => {
        if (rule.kind === "count" || rule.kind === "band") return removed.has(rule.piece_id) ? [] : [rule];
        if (rule.kind === "exclude") return removed.has(rule.a) || removed.has(rule.b) ? [] : [rule];
        const piece_ids = rule.piece_ids.filter((id) => !removed.has(id));
        return piece_ids.length ? [{ ...rule, piece_ids }] : [];
      }),
    })),
  };
}

export function slotSizeMigrationOffenders(project: WfcProject, slotW: number, slotH: number): WfcPiece[] {
  return project.pieces.filter((piece) => piece.tw % slotW !== 0 || piece.th % slotH !== 0);
}

export function socketKindLabel(kind: SocketKind): string {
  return typeof kind === "string" ? "symmetric" : `directed → ${kind.directed.partner}`;
}
