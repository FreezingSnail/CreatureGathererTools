// Editor-domain types mirroring the backend model (crates/core/src/model.rs).
// Kept separate from the shared types.ts (owned by the map-editor task) to
// avoid clashing with that in-flight work.

export interface MoveEntry {
  id: number;
  name: string;
  type: string;
  power: number;
  physical: number;
  accuracy: number;
  effect: string;
}

export interface Creature {
  name: string;
  id: number;
  type1: string;
  type2: string;
  evelevel: number;
  atk: number;
  deff: number;
  spcatk: number;
  spcdef: number;
  hp: number;
  spd: number;
  move1: number;
  move2: number;
  move3: number;
  move4: number;
  moveList: MoveEntry[];
}

export interface Move {
  id: number;
  name: string;
  type: string;
  power: number;
  physical: number;
  accuracy: number;
  effect: string;
}

export const CREATURE_TYPES = [
  "spirit",
  "water",
  "wind",
  "earth",
  "fire",
  "lightning",
  "plant",
  "elder",
  "none",
] as const;

export const MOVE_TYPES = [...CREATURE_TYPES, "status"] as const;

export const MOVE_EFFECTS = [
  "NONE",
  "SOAKED",
  "SPDDWN",
  "CONCUSED",
  "KINDLD",
  "CHRGD",
  "SAPPD",
  "DRNCHD",
  "PINNED",
  "ATKUP",
  "SPDUP",
  "ATKDWN",
  "DEFUP",
  "SPCAUP",
  "INFSED",
] as const;