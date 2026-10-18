import { describe, expect, it } from "vitest";
import {
  emptyCreature,
  emptyMove,
  hasFormErrors,
  maxId,
  parseNonNegativeInt,
  validateCreatureForm,
  validateCreatureRecord,
  validateMoveForm,
  validateMoveRecord,
} from "./validation";

describe("parseNonNegativeInt", () => {
  it("parses plain integers", () => {
    expect(parseNonNegativeInt("0")).toBe(0);
    expect(parseNonNegativeInt("42")).toBe(42);
    expect(parseNonNegativeInt("  7 ")).toBe(7);
  });

  it("rejects negatives", () => {
    expect(parseNonNegativeInt("-1")).toBeNull();
    expect(parseNonNegativeInt("-0")).toBeNull();
  });

  it("rejects malformed input", () => {
    expect(parseNonNegativeInt("")).toBeNull();
    expect(parseNonNegativeInt("  ")).toBeNull();
    expect(parseNonNegativeInt("1.5")).toBeNull();
    expect(parseNonNegativeInt("abc")).toBeNull();
    expect(parseNonNegativeInt("1e3")).toBeNull();
    expect(parseNonNegativeInt("999999999999999999999999")).toBeNull();
  });
});

describe("validateCreatureForm", () => {
  const good = {
    name: "Floofer",
    evelevel: "12",
    hp: "5",
  };

  it("accepts a complete valid form", () => {
    const e = validateCreatureForm(good);
    expect(hasFormErrors(e)).toBe(false);
  });

  it("rejects empty name", () => {
    const e = validateCreatureForm({ ...good, name: "   " });
    expect(e.name).toBe("Name must not be empty");
  });

  it("rejects a negative HP seed", () => {
    const e = validateCreatureForm({ ...good, hp: "-3" });
    expect(e.hp).toBe("HP must be a non-negative integer");
  });

  it("rejects non-integer stats", () => {
    const e = validateCreatureForm({ ...good, hp: "12.5" });
    expect(e.hp).toBe("HP must be a non-negative integer");
  });
});

describe("validateMoveForm", () => {
  const good = { name: "Tackle", power: "40", accuracy: "100" };

  it("accepts a complete valid form", () => {
    expect(hasFormErrors(validateMoveForm(good))).toBe(false);
  });

  it("rejects empty name and negative accuracy", () => {
    const e = validateMoveForm({ ...good, name: "", accuracy: "-5" });
    expect(e.name).toBe("Name must not be empty");
    expect(e.accuracy).toBe("Accuracy must be a non-negative integer");
  });
});

describe("record-level validation", () => {
  it("validates a full creature record", () => {
    const c = emptyCreature(3);
    expect(validateCreatureRecord(c)).toEqual([]);
    expect(validateCreatureRecord({ ...c, hp: -1 })).toContain(
      "hp not a non-negative integer"
    );
    expect(validateCreatureRecord({ ...c, name: "" })).toContain("creature name empty");
  });

  it("validates a full move record", () => {
    const m = emptyMove(1);
    expect(validateMoveRecord(m)).toEqual([]);
    expect(validateMoveRecord({ ...m, physical: 2 })).toContain("physical must be 0 or 1");
    expect(validateMoveRecord({ ...m, power: -10 })).toContain(
      "power not a non-negative integer"
    );
  });
});

describe("factories and helpers", () => {
  it("creates a creature with defaulted fields", () => {
    const c = emptyCreature(9);
    expect(c.id).toBe(9);
    expect(c.name).toBe("new_creature_9");
    expect(c.type1).toBe("none");
    expect(c.moves).toEqual([]);
  });

  it("creates a move with defaults", () => {
    const m = emptyMove(5);
    expect(m.id).toBe(5);
    expect(m.name).toBe("new_move_5");
    expect(m.accuracy).toBe(100);
    expect(m.effect).toBe("NONE");
  });

  it("computes max id (or -1 when empty)", () => {
    expect(maxId([{ id: 2 }, { id: 10 }, { id: 4 }])).toBe(10);
    expect(maxId([])).toBe(-1);
  });
});