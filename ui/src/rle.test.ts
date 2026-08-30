import { describe, expect, it } from "vitest";
import { decode, encode } from "./rle";

describe("RLE codec", () => {
  it("round-trips empty, single, identical, and alternating tiles", () => {
    for (const tiles of [[], [6], [6, 6, 6, 6], [1, 2, 1, 2, 1, 2]]) {
      expect(decode(encode(tiles), tiles.length)).toEqual(tiles);
    }
  });

  it("round-trips a 65536-tile run", () => {
    const tiles = new Array<number>(65536).fill(7);
    expect(encode(tiles)).toBe("7x65536");
    expect(decode(encode(tiles), tiles.length)).toEqual(tiles);
  });

  it("matches a fixture produced by the Rust encoder", () => {
    const fixture = "1x2,2,3x3,65535";
    const tiles = [1, 1, 2, 3, 3, 3, 65535];
    expect(encode(tiles)).toBe(fixture);
    expect(decode(fixture, tiles.length)).toEqual(tiles);
  });

  it.each(["6x0", "abc", "6x", "6,", "1,2"]) (
    "rejects malformed or mismatched input %j",
    (text) => {
      expect(() => decode(text, text === "1,2" ? 3 : 1)).toThrow();
    }
  );
});
