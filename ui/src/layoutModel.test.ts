import { describe, expect, it } from "vitest";
import {
  aggregateByNamespace,
  aggregateBySource,
  analyzeLayout,
  assignColorSlots,
  fieldPageRange,
  formatAddress,
  splitLayoutSections,
} from "./layoutModel";
import type { LayoutField, LayoutReport } from "./types";

function field(overrides: Partial<LayoutField> = {}): LayoutField {
  return {
    name: "title",
    namespace: "maps",
    addr: 0,
    len: 16,
    span: 16,
    kind: "bytes",
    in_save: false,
    source: "raw",
    path: "maps/title.bin",
    ...overrides,
  };
}

function report(fields: LayoutField[] = []): LayoutReport {
  return {
    data_page: 0,
    save_page: 100,
    data_len: 0,
    data_len_flash: 0,
    save_len: 0,
    save_len_flash: 0,
    page_size: 256,
    flash_pages: 100,
    fields,
  };
}

describe("layout model", () => {
  it("handles an empty report", () => {
    const sections = splitLayoutSections(report());
    expect(sections).toEqual({ data: [], save: [] });
    expect(aggregateByNamespace(sections.data)).toEqual([]);
    expect(aggregateBySource(sections.save)).toEqual([]);
    expect(analyzeLayout(sections.data)).toEqual({ gaps: [], padding: [], issues: [] });
  });

  it("maps a single field to inclusive 256-byte pages and formats addresses", () => {
    const entry = field({ addr: 0xfe, span: 4 });
    expect(fieldPageRange(entry)).toEqual({ first: 0, last: 1, count: 2 });
    expect(formatAddress(entry.addr)).toBe("0x0000FE");
    expect(analyzeLayout([entry])).toEqual({ gaps: [], padding: [], issues: [] });
  });

  it("reports padded space and byte shares", () => {
    const first = field({ addr: 0, len: 10, span: 16, namespace: "maps", source: "raw" });
    const second = field({ addr: 32, len: 30, span: 30, namespace: "sprites", source: "image" });
    const analysis = analyzeLayout([first, second]);
    expect(analysis.padding).toMatchObject([{ start: 10, end: 16, len: 6 }]);
    expect(analysis.gaps).toMatchObject([{ start: 16, end: 32, len: 16 }]);
    expect(aggregateByNamespace([first, second])).toMatchObject([
      { key: "maps", bytes: 10, share: 0.25, colorSlot: 0 },
      { key: "sprites", bytes: 30, share: 0.75, colorSlot: 1 },
    ]);
    expect(aggregateBySource([first, second])).toMatchObject([
      { key: "image", bytes: 30, share: 0.75 },
      { key: "raw", bytes: 10, share: 0.25 },
    ]);
  });

  it("separates save fields before layout analysis", () => {
    const data = field({ addr: 0 });
    const save = field({ addr: 0, in_save: true, namespace: "save" });
    expect(splitLayoutSections(report([data, save]))).toEqual({ data: [data], save: [save] });
  });

  it("guards against out-of-order and overlapping fields", () => {
    const first = field({ name: "first", addr: 32, span: 16 });
    const earlier = field({ name: "earlier", addr: 16, span: 16 });
    const overlap = field({ name: "overlap", addr: 24, span: 16 });
    expect(analyzeLayout([first, earlier, overlap]).issues.map((issue) => issue.kind)).toEqual([
      "out-of-order",
      "overlap",
    ]);
  });

  it("assigns slots independently of input order", () => {
    expect(assignColorSlots(["sprites", "maps", "sprites"])).toEqual({ maps: 0, sprites: 1 });
  });
});
