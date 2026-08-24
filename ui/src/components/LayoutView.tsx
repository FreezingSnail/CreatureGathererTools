import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  aggregateByNamespace,
  analyzeLayout,
  fieldPageRange,
  formatAddress,
  splitLayoutSections,
  type LayoutGroup,
} from "../layoutModel";
import type { LayoutField, LayoutReport } from "../types";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { Input } from "./ui/input";

const GROUP_COLORS = ["#0ea5e9", "#8b5cf6", "#f97316", "#10b981", "#ec4899", "#eab308"];
const PADDING_COLOR = "#94a3b8";
const SAVE_SECTOR_BYTES = 4096;

type SortKey = "name" | "namespace" | "addr" | "len" | "span" | "kind";
type SortDirection = "asc" | "desc";

interface PageStripProps {
  label: string;
  fields: readonly LayoutField[];
  groups: readonly LayoutGroup[];
  selected: LayoutField | null;
  onSelect: (field: LayoutField) => void;
}

function groupColor(groups: readonly LayoutGroup[], field: LayoutField) {
  const slot = groups.find((group) => group.key === field.namespace)?.colorSlot ?? 0;
  return GROUP_COLORS[slot % GROUP_COLORS.length];
}

function PageStrip({ label, fields, groups, selected, onSelect }: PageStripProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [hovered, setHovered] = useState<LayoutField | null>(null);
  const analysis = useMemo(() => analyzeLayout(fields), [fields]);
  const pageSize = 256;
  const cell = 24;
  const columns = 16;
  const ranges = useMemo(
    () => fields.map((field) => ({ field, range: fieldPageRange(field, pageSize) })),
    [fields]
  );
  const pageCount = Math.max(1, ...ranges.map(({ range }) => range.last + 1));
  const rows = Math.ceil(pageCount / columns);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    context.clearRect(0, 0, canvas.width, canvas.height);
    context.fillStyle = "#e2e8f0";
    context.fillRect(0, 0, canvas.width, canvas.height);
    for (let page = 0; page < pageCount; page++) {
      const x = (page % columns) * cell;
      const y = Math.floor(page / columns) * cell;
      context.strokeStyle = "#cbd5e1";
      context.strokeRect(x + 0.5, y + 0.5, cell - 1, cell - 1);
    }
    for (const { field, range } of ranges) {
      context.fillStyle = groupColor(groups, field);
      for (let page = range.first; page <= range.last; page++) {
        const x = (page % columns) * cell;
        const y = Math.floor(page / columns) * cell;
        context.fillRect(x + 2, y + 2, cell - 4, cell - 4);
      }
    }
    for (const padding of analysis.padding) {
      const range = fieldPageRange({ addr: padding.start, span: padding.len }, pageSize);
      context.fillStyle = PADDING_COLOR;
      for (let page = range.first; page <= range.last; page++) {
        const x = (page % columns) * cell;
        const y = Math.floor(page / columns) * cell;
        context.fillRect(x + cell - 6, y + 2, 4, cell - 4);
      }
    }
    if (selected) {
      const range = fieldPageRange(selected, pageSize);
      context.strokeStyle = "#111827";
      context.lineWidth = 2;
      for (let page = range.first; page <= range.last; page++) {
        const x = (page % columns) * cell;
        const y = Math.floor(page / columns) * cell;
        context.strokeRect(x + 1, y + 1, cell - 2, cell - 2);
      }
    }
  }, [analysis.padding, groups, pageCount, ranges, selected]);

  const fieldAt = (event: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = event.currentTarget;
    const bounds = canvas.getBoundingClientRect();
    const scaleX = canvas.width / bounds.width;
    const scaleY = canvas.height / bounds.height;
    const page =
      Math.floor(((event.clientY - bounds.top) * scaleY) / cell) * columns +
      Math.floor(((event.clientX - bounds.left) * scaleX) / cell);
    return ranges.find(({ range }) => page >= range.first && page <= range.last)?.field ?? null;
  };

  const canvasWidth = columns * cell;
  return (
    <div className="min-w-0">
      <div className="mb-1 flex items-center justify-between gap-2">
        <span className="text-xs font-medium">{label}</span>
        <span className="text-[11px] text-muted-foreground">256-byte pages</span>
      </div>
      <canvas
        ref={canvasRef}
        width={canvasWidth}
        height={Math.max(1, rows) * cell}
        className="block max-w-full cursor-pointer rounded border bg-slate-200"
        aria-label={`${label} flash page map`}
        data-selected-name={selected?.name ?? ""}
        onMouseMove={(event) => setHovered(fieldAt(event))}
        onMouseLeave={() => setHovered(null)}
        onClick={(event) => {
          const field = fieldAt(event);
          if (field) onSelect(field);
        }}
      />
      <p className="mt-1 min-h-4 text-[11px] text-muted-foreground" role="tooltip">
        {hovered
          ? `${hovered.name} · ${formatAddress(hovered.addr)} · ${hovered.len} B · ${hovered.kind}`
          : "Select a field from the table or map."}
      </p>
    </div>
  );
}

function SizeBars({ groups }: { groups: readonly LayoutGroup[] }) {
  if (groups.length === 0) return <p className="text-xs text-muted-foreground">No field payloads.</p>;
  return (
    <div className="space-y-1.5">
      {groups.map((group) => (
        <div key={group.key} className="grid grid-cols-[minmax(5rem,1fr)_3rem] items-center gap-2 text-xs">
          <div className="min-w-0">
            <div className="mb-0.5 flex justify-between gap-2">
              <span className="truncate">{group.key}</span>
              <span className="text-muted-foreground">{group.bytes} B · {(group.share * 100).toFixed(1)}%</span>
            </div>
            <div className="h-1.5 overflow-hidden rounded bg-muted">
              <div
                className="h-full rounded"
                style={{ width: `${group.share * 100}%`, backgroundColor: GROUP_COLORS[group.colorSlot % GROUP_COLORS.length] }}
              />
            </div>
          </div>
          <Badge variant="outline" className="justify-center">{group.fields.length}</Badge>
        </div>
      ))}
    </div>
  );
}

function LayoutTable({ fields, selected, onSelect }: { fields: readonly LayoutField[]; selected: LayoutField | null; onSelect: (field: LayoutField) => void }) {
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<SortKey>("addr");
  const [direction, setDirection] = useState<SortDirection>("asc");
  const query = filter.trim().toLocaleLowerCase();
  const ordered = [...fields]
    .filter((field) => !query || [field.name, field.namespace, field.kind, field.source].join(" ").toLocaleLowerCase().includes(query))
    .sort((left, right) => {
      const a = left[sort];
      const b = right[sort];
      const comparison = typeof a === "number" && typeof b === "number" ? a - b : String(a).localeCompare(String(b));
      return direction === "asc" ? comparison : -comparison;
    });
  const changeSort = (key: SortKey) => {
    if (sort === key) setDirection((current) => (current === "asc" ? "desc" : "asc"));
    else {
      setSort(key);
      setDirection("asc");
    }
  };
  const header = (label: string, key: SortKey) => (
    <th className="whitespace-nowrap px-2 py-1 text-left font-medium">
      <Button type="button" variant="ghost" size="sm" className="h-6 px-1 text-xs" onClick={() => changeSort(key)}>
        {label}{sort === key ? (direction === "asc" ? " ↑" : " ↓") : ""}
      </Button>
    </th>
  );

  return (
    <Card className="gap-0 py-0 shadow-none">
      <CardHeader className="flex-row items-center gap-2 px-2.5 py-2">
        <CardTitle className="text-sm">Fields <Badge variant="secondary">{ordered.length}</Badge></CardTitle>
        <Input aria-label="Filter fields" value={filter} onChange={(event) => setFilter(event.target.value)} placeholder="Filter fields…" className="ml-auto h-7 max-w-52 text-xs" />
      </CardHeader>
      <CardContent className="overflow-auto px-0 pb-0">
        <table className="w-full min-w-[780px] border-collapse text-xs" aria-label="Layout fields">
          <thead className="sticky top-0 bg-card">
            <tr className="border-y">{header("Name", "name")}{header("Namespace", "namespace")}{header("Address", "addr")}{header("Length", "len")}{header("Span", "span")}<th className="px-2 py-1 text-left font-medium">Pages</th>{header("Kind", "kind")}<th className="px-2 py-1 text-left font-medium">Image</th></tr>
          </thead>
          <tbody>
            {ordered.map((field, index) => {
              const pages = fieldPageRange(field);
              const image = field.image ? `${field.image.width}×${field.image.height} × ${field.image.frames}` : "—";
              return <tr key={`${field.in_save}-${field.name}-${field.addr}-${index}`} aria-selected={selected === field} className="cursor-pointer border-b hover:bg-muted/50 aria-selected:bg-primary/10" onClick={() => onSelect(field)}>
                <td className="px-2 py-1 font-medium">{field.name}</td><td className="px-2 py-1">{field.namespace}</td><td className="px-2 py-1 font-mono">{formatAddress(field.addr)}</td><td className="px-2 py-1">{field.len} B</td><td className="px-2 py-1">{field.span} B</td><td className="px-2 py-1">{pages.count ? `${pages.first}–${pages.last}` : "—"}</td><td className="px-2 py-1"><Badge variant="outline">{field.kind}</Badge></td><td className="px-2 py-1">{image}</td>
              </tr>;
            })}
          </tbody>
        </table>
      </CardContent>
    </Card>
  );
}

export function LayoutView() {
  const [report, setReport] = useState<LayoutReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [selected, setSelected] = useState<LayoutField | null>(null);
  const refresh = async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await invoke<LayoutReport>("layout_report");
      setReport({ ...next, fields: next.fields.map((field) => ({ ...field, namespace: field.namespace ?? "root" })) });
      setSelected(null);
    } catch (reason) {
      setReport(null);
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };
  useEffect(() => { void refresh(); }, []);

  if (loading) return <div data-testid="layout-loading" className="p-4 text-sm text-muted-foreground">Loading layout…</div>;
  if (error) {
    const missingBuilderBins = error.includes("missing builder");
    return <div className="p-4"><Alert variant="destructive" data-testid="layout-error"><AlertTitle>Layout unavailable</AlertTitle><AlertDescription>{error}{missingBuilderBins && <><br />Run <strong>Compile all</strong> to generate the required builder bins, then refresh.</>}</AlertDescription></Alert><Button size="sm" variant="outline" className="mt-2" onClick={() => void refresh()}>Refresh</Button></div>;
  }
  if (!report || report.fields.length === 0) return <div className="p-4"><Alert data-testid="layout-empty"><AlertTitle>No layout fields</AlertTitle><AlertDescription>Compile project content, then refresh the layout report.</AlertDescription></Alert><Button size="sm" variant="outline" className="mt-2" onClick={() => void refresh()}>Refresh</Button></div>;

  const sections = splitLayoutSections(report);
  const dataGroups = aggregateByNamespace(sections.data);
  const saveGroups = aggregateByNamespace(sections.save);
  const dataAnalysis = analyzeLayout(sections.data);
  const saveAnalysis = analyzeLayout(sections.save);
  const paddingBytes = [...dataAnalysis.padding, ...saveAnalysis.padding].reduce((total, padding) => total + padding.len, 0);
  const pagesUsed = fieldPageRange({ addr: 0, span: report.data_len_flash }, report.page_size).count;
  const saveSectors = report.save_len_flash / SAVE_SECTOR_BYTES;

  return <div className="h-full overflow-auto p-2.5" data-testid="layout-view">
    <div className="mb-2 flex flex-wrap items-center gap-1.5">
      <span className="text-sm font-semibold">FX memory layout</span>
      <Badge variant="secondary">FX_DATA_PAGE {report.data_page}</Badge>
      <Badge variant="outline">Data {report.data_len} B</Badge>
      <Badge variant="outline">Save {report.save_len} B</Badge>
      <Badge variant="outline">{pagesUsed} pages used</Badge>
      <Badge variant="outline">{saveSectors} save sectors</Badge>
      <Button size="sm" variant="outline" className="ml-auto h-7 text-xs" onClick={() => void refresh()}>Refresh</Button>
    </div>
    <div className="mb-2 grid gap-2 xl:grid-cols-2">
      <Card className="gap-2 py-2 shadow-none"><CardHeader className="px-2.5 py-0"><CardTitle className="text-sm">Data address space</CardTitle></CardHeader><CardContent className="px-2.5"><PageStrip label="Data" fields={sections.data} groups={dataGroups} selected={selected} onSelect={setSelected} /></CardContent></Card>
      <Card className="gap-2 py-2 shadow-none"><CardHeader className="px-2.5 py-0"><CardTitle className="text-sm">Save address space</CardTitle></CardHeader><CardContent className="px-2.5"><PageStrip label="Save" fields={sections.save} groups={saveGroups} selected={selected} onSelect={setSelected} /></CardContent></Card>
    </div>
    <div className="mb-2 grid gap-2 lg:grid-cols-[2fr_1fr]"><Card className="gap-2 py-2 shadow-none"><CardHeader className="px-2.5 py-0"><CardTitle className="text-sm">Payload by namespace</CardTitle></CardHeader><CardContent className="px-2.5"><SizeBars groups={dataGroups} />{saveGroups.length > 0 && <div className="mt-2 border-t pt-2"><p className="mb-1 text-xs text-muted-foreground">Save</p><SizeBars groups={saveGroups} /></div>}</CardContent></Card><Card className="gap-2 py-2 shadow-none"><CardHeader className="px-2.5 py-0"><CardTitle className="text-sm">Alignment padding</CardTitle></CardHeader><CardContent className="px-2.5 text-xs"><p><strong>{paddingBytes} B</strong> reserved beyond payload spans.</p><p className="mt-1 text-muted-foreground">{dataAnalysis.padding.length + saveAnalysis.padding.length} padded fields; shown in gray on maps.</p></CardContent></Card></div>
    <LayoutTable fields={report.fields} selected={selected} onSelect={setSelected} />
  </div>;
}
