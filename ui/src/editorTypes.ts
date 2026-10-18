// Editor-domain types mirroring the backend model (crates/core/src/model.rs).
// Kept separate from the shared types.ts (owned by the map-editor task) to
// avoid clashing with that in-flight work.

export interface Creature {
  name: string;
  id: number;
  type1: string;
  type2: string;
  evelevel: number;
  hp: number;
  /**
   * Retired stat seeds (CreatureGathererFX-rhs7.3). The model and duel keep
   * only `hp`; these stay optional so stale older authoring fixtures still
   * typecheck, and the editor never writes them back.
   */
  atk?: number;
  deff?: number;
  spcatk?: number;
  spcdef?: number;
  spd?: number;
  /** Known attack types, authored 8-type names (e.g. ["spirit","water"]). */
  moves: string[];
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

/**
 * Duel types exposed as creature attack-type checkboxes, in the pinned fold
 * order (bit i of the 5-bit attack mask). See docs/moveset-type-mask-brief.md.
 */
export const DUEL_TYPES = ["spirit", "water", "earth", "fire", "plant"] as const;

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