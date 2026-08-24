// Pure FX memory-layout math for the Layout view. No React, DOM, or IPC here.

import type { LayoutField, LayoutReport } from "./types";

export const LAYOUT_PAGE_SIZE = 256;

export interface PageRange {
  first: number;
  last: number;
  count: number;
}

export interface LayoutGroup {
  key: string;
  bytes: number;
  share: number;
  colorSlot: number;
  fields: LayoutField[];
}

export interface LayoutGap {
  start: number;
  end: number;
  len: number;
  before: LayoutField;
  after: LayoutField;
}

export interface LayoutPadding {
  start: number;
  end: number;
  len: number;
  field: LayoutField;
}

export interface LayoutOrderIssue {
  kind: "out-of-order" | "overlap";
  previous: LayoutField;
  field: LayoutField;
}

export interface LayoutAnalysis {
  gaps: LayoutGap[];
  padding: LayoutPadding[];
  issues: LayoutOrderIssue[];
}

export interface LayoutSections {
  data: LayoutField[];
  save: LayoutField[];
}

/** Inclusive 256-byte page range occupied by a field's reserved span. */
export function fieldPageRange(
  field: Pick<LayoutField, "addr" | "span">,
  pageSize = LAYOUT_PAGE_SIZE
): PageRange {
  const size = Math.max(1, Math.trunc(pageSize));
  const span = Math.max(0, Math.trunc(field.span));
  if (span === 0) {
    return { first: Math.floor(field.addr / size), last: Math.floor(field.addr / size), count: 0 };
  }
  const first = Math.floor(field.addr / size);
  const last = Math.floor((field.addr + span - 1) / size);
  return { first, last, count: last - first + 1 };
}

/** `0x`-prefixed, at-least-six-digit uppercase hexadecimal address. */
export function formatAddress(addr: number): string {
  return `0x${Math.max(0, Math.trunc(addr)).toString(16).toUpperCase().padStart(6, "0")}`;
}

/** Deterministic color indices: identical group names receive identical slots. */
export function assignColorSlots(keys: readonly string[]): Record<string, number> {
  return Object.fromEntries(
    [...new Set(keys)].sort((a, b) => a.localeCompare(b)).map((key, colorSlot) => [key, colorSlot])
  );
}

function aggregate(fields: readonly LayoutField[], keyFor: (field: LayoutField) => string): LayoutGroup[] {
  const grouped = new Map<string, LayoutField[]>();
  for (const field of fields) {
    const key = keyFor(field);
    const entries = grouped.get(key) ?? [];
    entries.push(field);
    grouped.set(key, entries);
  }
  const slots = assignColorSlots([...grouped.keys()]);
  const total = fields.reduce((sum, field) => sum + Math.max(0, field.len), 0);
  return [...grouped.entries()]
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([key, entries]) => {
      const bytes = entries.reduce((sum, field) => sum + Math.max(0, field.len), 0);
      return { key, bytes, share: total === 0 ? 0 : bytes / total, colorSlot: slots[key], fields: entries };
    });
}

/** Groups payload bytes by namespace. */
export function aggregateByNamespace(fields: readonly LayoutField[]): LayoutGroup[] {
  return aggregate(fields, (field) => field.namespace);
}

/** Groups payload bytes by packer source kind. */
export function aggregateBySource(fields: readonly LayoutField[]): LayoutGroup[] {
  return aggregate(fields, (field) => field.source);
}

/** Data and save fields have independent address spaces and must not be interleaved. */
export function splitLayoutSections(report: Pick<LayoutReport, "fields">): LayoutSections {
  return {
    data: report.fields.filter((field) => !field.in_save),
    save: report.fields.filter((field) => field.in_save),
  };
}

/** Finds holes, trailing alignment bytes, and unsafe field order in one address space. */
export function analyzeLayout(fields: readonly LayoutField[]): LayoutAnalysis {
  const gaps: LayoutGap[] = [];
  const padding: LayoutPadding[] = [];
  const issues: LayoutOrderIssue[] = [];

  for (const field of fields) {
    const paddingLen = Math.max(0, field.span - field.len);
    if (paddingLen > 0) {
      padding.push({ start: field.addr + field.len, end: field.addr + field.span, len: paddingLen, field });
    }
  }

  for (let index = 1; index < fields.length; index++) {
    const previous = fields[index - 1];
    const field = fields[index];
    const previousEnd = previous.addr + Math.max(0, previous.span);
    if (field.addr < previous.addr) {
      issues.push({ kind: "out-of-order", previous, field });
    } else if (field.addr < previousEnd) {
      issues.push({ kind: "overlap", previous, field });
    } else if (field.addr > previousEnd) {
      gaps.push({ start: previousEnd, end: field.addr, len: field.addr - previousEnd, before: previous, after: field });
    }
  }

  return { gaps, padding, issues };
}
