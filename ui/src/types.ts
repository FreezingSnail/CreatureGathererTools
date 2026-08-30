// TypeScript mirrors of the backend GameMap model (crates/core/src/model.rs)
// and serialized IPC result types (src-tauri/src/lib.rs).

export interface Config {
  creatures_json: string;
  moves_json: string;
  map_json: string;
  tileset_png: string;
  map_tileset_png: string;
  npc_tileset_png: string;
  output_dir: string;
  fx_layout?: string;
}

export interface NPCObject {
  id: number;
  x: number;
  y: number;
  sprite_id: number;
  script: string;
}

export interface ScriptObject {
  id: number;
  x: number;
  y: number;
  script: string;
}

export interface CollisionType {
  id: number;
  name: string;
  color: string; // hex RGBA e.g. "#FF000080"
}

export interface EncounterZone {
  id: number;
  name: string;
  rate: number; // 0-100
  creature_ids: number[];
}

export type Side = "n" | "e" | "s" | "w";

export type SocketKind = "symmetric" | { directed: { partner: number } };

export interface EdgeSocket {
  side: Side;
  index: number;
  label: number;
}

export interface SocketLabel {
  id: number;
  name: string;
  kind: SocketKind;
}

export interface WfcPiece {
  id: number;
  name: string;
  tw: number;
  th: number;
  tiles: string;
  weight: number;
  limit?: number | null;
  edge_overrides: EdgeSocket[];
  tags: string[];
}

export interface Anchor {
  piece_id: number;
  sx: number;
  sy: number;
}

export type RuleSpec =
  | { kind: "count"; piece_id: number; min: number | null; max: number | null }
  | { kind: "exclude"; a: number; b: number; radius: number }
  | { kind: "border"; piece_ids: number[] }
  | { kind: "band"; piece_id: number; from_row: number; to_row: number };

export interface WfcRecipe {
  id: number;
  name: string;
  piece_ids: number[];
  size_w: number;
  size_h: number;
  seed: number;
  anchors: Anchor[];
  socket_compat: [number, number][];
  rules?: RuleSpec[];
}

export interface WfcProject {
  version: number;
  slot_w: number;
  slot_h: number;
  pieces: WfcPiece[];
  socket_labels: SocketLabel[];
  recipes: WfcRecipe[];
}

export interface StampOrigin {
  recipe_id: number;
  seed: number;
  slot_w: number;
  slot_h: number;
}

export interface Stamp {
  id: number;
  name: string;
  w: number;
  h: number;
  tiles: string;
  origin?: StampOrigin | null;
}

export interface Hunk {
  w: number;
  h: number;
  slot_w: number;
  slot_h: number;
  tiles: number[];
  slots: number[];
  seed: number;
  attempts: number;
}

export type ValidationIssue = string;

export interface TileRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface CaptureOpts {
  name: string;
  weight: number;
  limit?: number | null;
}

export interface GameMap {
  width: number;
  height: number;
  tiles: number[][];
  npcs: NPCObject[];
  scripts: ScriptObject[];
  collision: number[][]; // 0 = none, else CollisionType.ID
  collision_types: CollisionType[];
  encounters: number[][]; // -1 = none, else EncounterZone.ID
  encounter_zones: EncounterZone[];
  wfc?: WfcProject;
  stamps?: Stamp[];
}

export interface ScriptErr {
  line: number;
  col: number;
  msg: string;
}

export interface ParseResult {
  ok: boolean;
  errors: ScriptErr[];
  cmds: unknown[] | null;
}

export interface CompileError {
  line: number;
  msg: string;
}

export interface CompileResult {
  ok: boolean;
  errors: CompileError[];
  artifacts: string[];
}

export interface LayoutImage {
  frames: number;
  width: number;
  height: number;
}

export interface LayoutField {
  name: string;
  namespace: string;
  addr: number;
  len: number;
  span: number;
  kind: "bytes" | "image";
  in_save: boolean;
  source: "raw" | "carray" | "symbol" | "image" | "builder";
  path: string;
  image?: LayoutImage;
}

export interface LayoutReport {
  data_page: number;
  save_page: number;
  data_len: number;
  data_len_flash: number;
  save_len: number;
  save_len_flash: number;
  page_size: number;
  flash_pages: number;
  fields: LayoutField[];
}
