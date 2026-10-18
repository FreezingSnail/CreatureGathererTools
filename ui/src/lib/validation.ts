// Pure validation + parse helpers for the creature/move editors.
// No React, no Tauri — unit-testable in isolation.

import type { Creature, Move } from "../editorTypes";

/** Parse a non-negative integer field. Returns null when malformed. */
export function parseNonNegativeInt(raw: string): number | null {
  const trimmed = raw.trim();
  if (trimmed === "") return null;
  if (!/^\d+$/.test(trimmed)) return null;
  const n = Number(trimmed);
  if (!Number.isSafeInteger(n)) return null;
  return n;
}

/** Reject values that are not non-negative integers. */
export function validNonNegativeInt(raw: string): boolean {
  return parseNonNegativeInt(raw) !== null;
}

export function validNonEmpty(raw: string): boolean {
  return raw.trim().length > 0;
}

export interface CreatureFormErrors {
  name: string | null;
  evelevel: string | null;
  hp: string | null;
}

export function emptyCreatureFormErrors(): CreatureFormErrors {
  return {
    name: null,
    evelevel: null,
    hp: null,
  };
}

export function validateCreatureForm(f: {
  name: string;
  evelevel: string;
  hp: string;
}): CreatureFormErrors {
  return {
    name: validNonEmpty(f.name) ? null : "Name must not be empty",
    evelevel: validNonNegativeInt(f.evelevel)
      ? null
      : "Evo level must be a non-negative integer",
    hp: validNonNegativeInt(f.hp) ? null : "HP must be a non-negative integer",
  };
}

export function hasFormErrors(e: object): boolean {
  return Object.values(e).some((v) => v !== null);
}

export interface MoveFormErrors {
  name: string | null;
  power: string | null;
  accuracy: string | null;
}

export function emptyMoveFormErrors(): MoveFormErrors {
  return { name: null, power: null, accuracy: null };
}

export function validateMoveForm(f: {
  name: string;
  power: string;
  accuracy: string;
}): MoveFormErrors {
  return {
    name: validNonEmpty(f.name) ? null : "Name must not be empty",
    power: validNonNegativeInt(f.power)
      ? null
      : "Power must be a non-negative integer",
    accuracy: validNonNegativeInt(f.accuracy)
      ? null
      : "Accuracy must be a non-negative integer",
  };
}

/** Reject a full creature record. Returns list of human-readable problems. */
export function validateCreatureRecord(c: Creature): string[] {
  const errs: string[] = [];
  if (!validNonEmpty(c.name)) errs.push("creature name empty");
  if (c.id < 0) errs.push("creature id negative");
  if (!Number.isSafeInteger(c.id)) errs.push("creature id not an integer");
  for (const [key, val] of [
    ["evelevel", c.evelevel],
    ["hp", c.hp],
  ] as const) {
    if (!Number.isSafeInteger(val) || val < 0) errs.push(`${key} not a non-negative integer`);
  }
  for (const key of ["type1", "type2"] as const) {
    if (typeof c[key] !== "string" || c[key] === "") errs.push(`${key} empty`);
  }
  return errs;
}

export function validateMoveRecord(m: Move): string[] {
  const errs: string[] = [];
  if (!validNonEmpty(m.name)) errs.push("move name empty");
  if (!Number.isSafeInteger(m.id) || m.id < 0) errs.push("move id not a non-negative integer");
  if (!Number.isSafeInteger(m.power) || m.power < 0) errs.push("power not a non-negative integer");
  if (!Number.isSafeInteger(m.accuracy) || m.accuracy < 0)
    errs.push("accuracy not a non-negative integer");
  if (m.physical !== 0 && m.physical !== 1) errs.push("physical must be 0 or 1");
  if (!validNonEmpty(m.type)) errs.push("type empty");
  return errs;
}

export function emptyCreature(id: number): Creature {
  return {
    name: `new_creature_${id}`,
    id,
    type1: "none",
    type2: "none",
    evelevel: 0,
    hp: 0,
    moves: [],
  };
}

export function emptyMove(id: number): Move {
  return {
    id,
    name: `new_move_${id}`,
    type: "spirit",
    power: 0,
    physical: 0,
    accuracy: 100,
    effect: "NONE",
  };
}

export function maxId(items: Array<{ id: number }>): number {
  return items.reduce((max, it) => (it.id > max ? it.id : max), -1);
}