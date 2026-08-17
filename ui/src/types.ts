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