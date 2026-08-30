import { useEffect, useRef } from "react";
import type { GameMap, Stamp, TileRect } from "../types";
import type { TileClipboard } from "../state";
import { decode, encode } from "../rle";
import { Button } from "./ui/button";

export interface StampLibraryProps {
  map: GameMap;
  tiles: HTMLCanvasElement[];
  selection: TileRect | null;
  onMapChange: (map: GameMap) => void;
  onSetClipboard: (clipboard: TileClipboard) => void;
}

function stampTiles(stamp: Stamp): number[] {
  try {
    return decode(stamp.tiles, stamp.w * stamp.h);
  } catch {
    return [];
  }
}

function StampThumbnail({ stamp, tiles }: { stamp: Stamp; tiles: HTMLCanvasElement[] }) {
  const ref = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (!canvas || !ctx) return;
    const values = stampTiles(stamp);
    const scale = Math.max(1, Math.min(16, Math.floor(96 / Math.max(stamp.w, stamp.h, 1))));
    canvas.width = Math.max(1, stamp.w * scale);
    canvas.height = Math.max(1, stamp.h * scale);
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    values.forEach((tile, index) => {
      const source = tiles[tile];
      if (!source) return;
      const x = (index % stamp.w) * scale;
      const y = Math.floor(index / stamp.w) * scale;
      ctx.drawImage(source, x, y, scale, scale);
    });
  }, [stamp, tiles]);
  return <canvas ref={ref} aria-label={`${stamp.name} thumbnail`} style={{ imageRendering: "pixelated" }} />;
}

export function StampLibrary({ map, tiles, selection, onMapChange, onSetClipboard }: StampLibraryProps) {
  const stamps = map.stamps ?? [];
  const saveSelection = () => {
    if (!selection) return;
    const name = window.prompt("Stamp name", `stamp ${stamps.length + 1}`)?.trim();
    if (!name) return;
    const values: number[] = [];
    for (let y = selection.y; y < selection.y + selection.h; y += 1) {
      for (let x = selection.x; x < selection.x + selection.w; x += 1) {
        values.push(map.tiles[y]?.[x] ?? 0);
      }
    }
    const id = stamps.reduce((max, stamp) => Math.max(max, stamp.id), -1) + 1;
    const stamp: Stamp = {
      id,
      name,
      w: selection.w,
      h: selection.h,
      tiles: encode(values),
      origin: null,
    };
    onMapChange({ ...map, stamps: [...stamps, stamp] });
  };

  const rename = (stamp: Stamp) => {
    const name = window.prompt("Stamp name", stamp.name)?.trim();
    if (!name || name === stamp.name) return;
    onMapChange({
      ...map,
      stamps: stamps.map((item) => item.id === stamp.id ? { ...item, name } : item),
    });
  };

  const remove = (stamp: Stamp) => {
    onMapChange({ ...map, stamps: stamps.filter((item) => item.id !== stamp.id) });
  };

  const load = (stamp: Stamp) => {
    const values = stampTiles(stamp);
    if (values.length === stamp.w * stamp.h) {
      onSetClipboard({ w: stamp.w, h: stamp.h, tiles: values });
    }
  };

  return (
    <section data-testid="stamp-library" style={{ borderTop: "1px solid #333", padding: 8 }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 4 }}>
        <strong style={{ fontSize: 12 }}>Stamps</strong>
        <Button type="button" size="sm" variant="outline" disabled={!selection} onClick={saveSelection}>
          Save selection as stamp
        </Button>
      </div>
      {stamps.length === 0 && <div className="text-muted-foreground" style={{ fontSize: 11, marginTop: 6 }}>No saved stamps.</div>}
      <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 6 }}>
        {stamps.map((stamp) => (
          <article key={stamp.id} style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <StampThumbnail stamp={stamp} tiles={tiles} />
            <div style={{ minWidth: 0, flex: 1 }}>
              <div style={{ fontSize: 12, overflow: "hidden", textOverflow: "ellipsis" }}>{stamp.name}</div>
              <div className="text-muted-foreground" style={{ fontSize: 10 }}>{stamp.w}×{stamp.h}</div>
            </div>
            <Button type="button" size="icon-sm" variant="ghost" aria-label={`Load ${stamp.name}`} onClick={() => load(stamp)}>↧</Button>
            <Button type="button" size="icon-sm" variant="ghost" aria-label={`Rename ${stamp.name}`} onClick={() => rename(stamp)}>✎</Button>
            <Button type="button" size="icon-sm" variant="ghost" aria-label={`Delete ${stamp.name}`} onClick={() => remove(stamp)}>×</Button>
          </article>
        ))}
      </div>
    </section>
  );
}

export default StampLibrary;
