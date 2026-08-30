import { describe, expect, it } from "vitest";
import type { WfcPiece, WfcProject } from "./types";
import { deriveEdgeStrips, extractThumbnailTiles, legalNeighbourCounts } from "./wfc";

const piece = (id: number, tiles: string, tw = 1, th = 1): WfcPiece => ({
  id, name: `piece ${id}`, tw, th, tiles, weight: 1, edge_overrides: [], tags: [],
});

describe("wfc helpers", () => {
  it("derives all four asymmetric boundary strips in shared-cell order", () => {
    const strips = deriveEdgeStrips(piece(1, "1,2,3,4,5,6", 3, 2), 1, 1);
    expect(strips.n.map((edge) => edge.tiles)).toEqual([[1], [2], [3]]);
    expect(strips.e.map((edge) => edge.tiles)).toEqual([[3], [6]]);
    expect(strips.s.map((edge) => edge.tiles)).toEqual([[4], [5], [6]]);
    expect(strips.w.map((edge) => edge.tiles)).toEqual([[1], [4]]);
  });

  it("extracts the requested thumbnail subgrid", () => {
    expect(extractThumbnailTiles(piece(1, "1,2,3,4,5,6", 3, 2), 1, 0, 2, 2)).toEqual([2, 3, 5, 6]);
  });

  it("counts legal neighbours by matching facing derived strips", () => {
    const project: WfcProject = {
      version: 1, slot_w: 1, slot_h: 1,
      pieces: [piece(1, "1"), piece(2, "2"), piece(3, "1")],
      socket_labels: [], recipes: [],
    };
    expect(legalNeighbourCounts(project, project.pieces[0])).toEqual({ n: [1], e: [1], s: [1], w: [1] });
    expect(legalNeighbourCounts(project, project.pieces[1])).toEqual({ n: [0], e: [0], s: [0], w: [0] });
  });
});
