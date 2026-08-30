// Web canvas map editor — direct port of the Go fyne mapcanvas.go behavior:
// zoom/pan, tile+block brush, 5 layers, ghost preview, chunk grid overlay,
// selection outlines, coordinate labels.

import { useCallback, useEffect, useRef, useState } from "react";
import type { GameMap, TileRect } from "./types";
import type { LayerType, EditorTool, TileClipboard } from "./state";
import {
  TILE_SIZE,
  MARGIN_X,
  MARGIN_Y,
  CHUNK_W,
  CHUNK_H,
  clampRect,
  rectFromPoints,
  screenToTile,
  tileToPixel,
  parseHexColor,
  zoneColor,
} from "./geometry";

export interface MapCanvasProps {
  map: GameMap;
  tiles: HTMLCanvasElement[];
  npcTiles: HTMLCanvasElement[];
  layer: LayerType;
  tool: EditorTool;
  zoom: number;
  offsetX: number;
  offsetY: number;
  activeBlock: number[];
  blockW: number;
  blockH: number;
  selection?: TileRect | null;
  clipboard?: TileClipboard | null;
  selectedNpc: number;
  selectedScript: number;
  showChunks: boolean;
  showChunkHighlight: boolean;
  showCollision: boolean;
  showEncounters: boolean;
  showNpcOverlay: boolean;
  showScriptOverlay: boolean;
  onViewChange: (offsetX: number, offsetY: number, zoom?: number) => void;
  onWatchZoom: (zoom: number) => void;
  onPushUndo: () => void;
  onPaintBlock: (col: number, row: number) => void;
  onPaintCollision: (col: number, row: number) => void;
  onPaintEncounter: (col: number, row: number) => void;
  onPasteAt?: (x: number, y: number) => void;
  onSetSelection?: (rect: TileRect | null) => void;
  onClickNpc: (col: number, row: number) => void;
  onClickScript: (col: number, row: number) => void;
  onOpenScript: (id: number) => void;
  onRightClick: (col: number, row: number) => void;
}

type RGBA = [number, number, number, number];

function withAlpha(c: RGBA, a: number): RGBA {
  return [c[0], c[1], c[2], a];
}

function blendOver(dst: RGBA, src: RGBA): RGBA {
  const f = src[3] / 255;
  return [
    Math.round(dst[0] * (1 - f) + src[0] * f),
    Math.round(dst[1] * (1 - f) + src[1] * f),
    Math.round(dst[2] * (1 - f) + src[2] * f),
    255,
  ];
}

const GLYPHS = [
  [0b111, 0b101, 0b101, 0b101, 0b111],
  [0b010, 0b110, 0b010, 0b010, 0b111],
  [0b111, 0b001, 0b111, 0b100, 0b111],
  [0b111, 0b001, 0b111, 0b001, 0b111],
  [0b101, 0b101, 0b111, 0b001, 0b001],
  [0b111, 0b100, 0b111, 0b001, 0b111],
  [0b111, 0b100, 0b111, 0b101, 0b111],
  [0b111, 0b001, 0b001, 0b001, 0b001],
  [0b111, 0b101, 0b111, 0b101, 0b111],
  [0b111, 0b101, 0b111, 0b001, 0b111],
];

function drawLabel(
  ctx: CanvasRenderingContext2D,
  str: string,
  cx: number,
  cy: number,
  scale: number,
  color: string
) {
  scale = Math.max(1, Math.floor(scale));
  const charW = 3 * scale;
  const charH = 5 * scale;
  const gap = scale;
  const totalW = str.length * (charW + gap) - gap;
  const x0 = cx - totalW / 2;
  const y0 = cy - charH / 2;
  ctx.fillStyle = color;
  for (let i = 0; i < str.length; i++) {
    const ch = str.charCodeAt(i);
    if (ch < 0x30 || ch > 0x39) {
      continue;
    }
    const g = GLYPHS[ch - 0x30];
    const ox = x0 + i * (charW + gap);
    for (let row = 0; row < 5; row++) {
      for (let bit = 0; bit < 3; bit++) {
        if (((g[row] >> (2 - bit)) & 1) === 1) {
          ctx.fillRect(
            ox + bit * scale,
            y0 + row * scale,
            scale,
            scale
          );
        }
      }
    }
  }
}

const GRID_COLOR = "rgba(40,40,40,0.47)";
/** Pixels of travel before a left-drag becomes a pan instead of a click. */
const DRAG_THRESHOLD = 4;

export function MapCanvas(props: MapCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [ghost, setGhost] = useState<{ col: number; row: number; visible: boolean }>({
    col: 0,
    row: 0,
    visible: false,
  });
  const [dragSelection, setDragSelection] = useState<TileRect | null>(null);
  const dragSelectionRef = useRef<TileRect | null>(null);
  dragSelectionRef.current = dragSelection;

  // Latest props/ghost via refs so the resize path can redraw without
  // re-registering observers on every state change.
  const propsRef = useRef(props);
  propsRef.current = props;
  const ghostRef = useRef(ghost);
  ghostRef.current = ghost;

  const drawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return; // jsdom-safe
    }
    drawFrame(ctx, canvas.width, canvas.height, propsRef.current, {
      ...ghostRef.current,
      selection: dragSelectionRef.current,
    });
  }, []);

  // canvas sizing + draw. Setting width/height wipes the buffer, so redraw or
// the map vanishes. During rapid resize (palette panel drag) don't redraw
// every tick: cheaply CSS-scale the existing bitmap (no wipe, GPU smooth),
// then do one crisp full redraw ~80ms after resize settles.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    let raf = 0;
    let debounce: ReturnType<typeof setTimeout> | undefined;
    const redraw = () => {
      raf = 0;
      const canvas = canvasRef.current;
      if (!canvas) {
        return;
      }
      canvas.width = container.clientWidth;
      canvas.height = container.clientHeight;
      drawCanvas();
    };
    const scheduleRedraw = () => {
      if (raf) {
        return;
      }
      raf = requestAnimationFrame(redraw);
    };
    const onResize = () => {
      const canvas = canvasRef.current;
      if (!canvas) {
        return;
      }
      canvas.style.width = `${container.clientWidth}px`;
      canvas.style.height = `${container.clientHeight}px`;
      clearTimeout(debounce);
      debounce = setTimeout(scheduleRedraw, 80);
    };
    onResize();
    if (typeof ResizeObserver === "undefined") {
      return;
    }
    const ro = new ResizeObserver(onResize);
    ro.observe(container);
    return () => {
      ro.disconnect();
      if (raf) {
        cancelAnimationFrame(raf);
      }
      clearTimeout(debounce);
    };
  }, [drawCanvas]);

  useEffect(() => {
    drawCanvas();
  }, [
    drawCanvas,
    props.map,
    props.tiles,
    props.npcTiles,
    props.zoom,
    props.offsetX,
    props.offsetY,
    props.layer,
    props.activeBlock,
    props.blockW,
    props.blockH,
    props.selection,
    props.clipboard,
    props.selectedNpc,
    props.selectedScript,
    props.showChunks,
    props.showChunkHighlight,
    props.showCollision,
    props.showEncounters,
    props.showNpcOverlay,
    props.showScriptOverlay,
    ghost,
    dragSelection,
  ]);

  const viewport = {
    zoom: props.zoom,
    offsetX: props.offsetX,
    offsetY: props.offsetY,
  };
  const startRef = useRef<{
    x: number;
    y: number;
    ox: number;
    oy: number;
    pending: { col: number; row: number } | null;
    selectionStart: { col: number; row: number } | null;
  } | null>(null);
  const panningRef = useRef(false);

  const toTile = (px: number, py: number) =>
    screenToTile(px, py, viewport.offsetX, viewport.offsetY, viewport.zoom);

  const paintAt = (col: number, row: number) => {
    switch (props.layer) {
      case "map":
        props.onPaintBlock(col, row);
        break;
      case "collision":
        props.onPaintCollision(col, row);
        break;
      case "encounter":
        props.onPaintEncounter(col, row);
        break;
      default:
        break;
    }
  };

  const clickAt = (col: number, row: number) => {
    switch (props.tool) {
      case "paste":
        props.onPasteAt?.(col, row);
        return;
      case "select":
        return;
      default:
        break;
    }
    switch (props.layer) {
      case "map":
      case "collision":
      case "encounter":
        props.onPushUndo();
        paintAt(col, row);
        break;
      case "npc":
        props.onClickNpc(col, row);
        break;
      case "script":
        props.onClickScript(col, row);
        break;
      default:
        break;
    }
  };

  const handleMouseDown = (e: React.MouseEvent) => {
    if (e.button === 2) {
      return; // handled by contextmenu
    }
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const middlePan = e.button === 1;
    const altPan = e.altKey;
    const panTool = props.tool === "pan";
    if (middlePan || altPan || panTool) {
      e.preventDefault();
      panningRef.current = true;
      startRef.current = {
        x,
        y,
        ox: props.offsetX,
        oy: props.offsetY,
        pending: null,
        selectionStart: null,
      };
      return;
    }
    const { col, row } = toTile(x, y);
    if (props.tool === "select") {
      startRef.current = {
        x,
        y,
        ox: props.offsetX,
        oy: props.offsetY,
        pending: null,
        selectionStart: { col, row },
      };
      setDragSelection(clampRect({ x: col, y: row, w: 1, h: 1 }, props.map.width, props.map.height));
      return;
    }
    startRef.current = {
      x,
      y,
      ox: props.offsetX,
      oy: props.offsetY,
      pending: { col, row },
      selectionStart: null,
    };
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    const { col, row } = toTile(x, y);
    setGhost({ col, row, visible: true });

    if (startRef.current?.selectionStart && props.tool === "select") {
      const next = clampRect(
        rectFromPoints(startRef.current.selectionStart, { col, row }),
        props.map.width,
        props.map.height
      );
      setDragSelection(next);
      return;
    }

    if (panningRef.current && startRef.current) {
      const tile = TILE_SIZE * props.zoom;
      const dx = Math.round((x - startRef.current.x) / tile);
      const dy = Math.round((y - startRef.current.y) / tile);
      props.onViewChange(startRef.current.ox - dx, startRef.current.oy - dy);
      return;
    }
    // left-drag beyond a small threshold becomes a pan (paint tool only; the
    // pan tool pans from the first mousemove)
    if (startRef.current?.pending && !panningRef.current && props.tool === "paint") {
      const dx = x - startRef.current.x;
      const dy = y - startRef.current.y;
      if (Math.abs(dx) > DRAG_THRESHOLD || Math.abs(dy) > DRAG_THRESHOLD) {
        panningRef.current = true;
      }
    }
  };

  const endDrag = () => {
    const start = startRef.current;
    if (start?.selectionStart && props.tool === "select") {
      props.onSetSelection?.(dragSelectionRef.current);
      setDragSelection(null);
    } else if (start?.pending && !panningRef.current) {
      clickAt(start.pending.col, start.pending.row);
    }
    panningRef.current = false;
    startRef.current = null;
  };

  const panAccRef = useRef({ x: 0, y: 0 });
  const zoomAccRef = useRef(0);

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const tile = TILE_SIZE * props.zoom;
    // ctrl/meta+wheel = pinch zoom (browser convention for trackpad pinch;
    // also the mouse-user fallback). ~100px of pinch delta = one old 1.2x
    // notch; leftovers accumulate so fast trackpad pinches feel snappy too.
    if (e.ctrlKey || e.metaKey) {
      zoomAccRef.current += e.deltaY;
      let z = props.zoom;
      let changed = false;
      while (zoomAccRef.current >= 100) {
        z /= 1.2;
        zoomAccRef.current -= 100;
        changed = true;
      }
      while (zoomAccRef.current <= -100) {
        z *= 1.2;
        zoomAccRef.current += 100;
        changed = true;
      }
      if (changed) {
        props.onWatchZoom(z);
      }
      return;
    }
    // plain wheel / two-finger scroll pans the view, accumulating leftover
    // pixels so trackpad micro-scrolls stay smooth instead of tile-snapping
    // on every event.
    const acc = panAccRef.current;
    acc.x += e.deltaX;
    acc.y += e.deltaY;
    let dx = 0;
    let dy = 0;
    while (acc.x >= tile) {
      dx += 1;
      acc.x -= tile;
    }
    while (acc.x <= -tile) {
      dx -= 1;
      acc.x += tile;
    }
    while (acc.y >= tile) {
      dy += 1;
      acc.y -= tile;
    }
    while (acc.y <= -tile) {
      dy -= 1;
      acc.y += tile;
    }
    if (dx !== 0 || dy !== 0) {
      props.onViewChange(props.offsetX + dx, props.offsetY + dy);
    }
  };

  const handleContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    const { col, row } = toTile(e.clientX - rect.left, e.clientY - rect.top);
    props.onRightClick(col, row);
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.preventDefault();
    if (props.layer !== "script") {
      return;
    }
    const rect = canvasRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    const { col, row } = toTile(e.clientX - rect.left, e.clientY - rect.top);
    const idx = props.map.scripts.findIndex((s) => s.x === col && s.y === row);
    if (idx >= 0) {
      props.onOpenScript(idx);
    }
  };

  return (
    <div
      ref={containerRef}
      style={{
        flex: 1,
        minHeight: 400,
        position: "relative",
        overflow: "hidden",
        background: "#1a1a1a",
        cursor:
          props.tool === "pan" ? "grab" : props.tool === "paste" ? "copy" : "crosshair",
      }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={endDrag}
      onMouseLeave={() => {
        endDrag();
        setGhost((g) => ({ ...g, visible: false }));
      }}
      onWheel={handleWheel}
      onContextMenu={handleContextMenu}
      onDoubleClick={handleDoubleClick}
      onAuxClick={(e) => e.preventDefault()}
    >
      <canvas
        ref={canvasRef}
        style={{ display: "block", width: "100%", height: "100%" }}
      />
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────
// Frame drawing (pure; separated so it can be exercised without React)
// ─────────────────────────────────────────────────────────────────────

export function drawFrame(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  p: MapCanvasProps,
  ghost: { col: number; row: number; visible: boolean; selection?: TileRect | null }
) {
  ctx.imageSmoothingEnabled = true;
  ctx.clearRect(0, 0, w, h);
  const m = p.map;
  const tileW = Math.max(1, Math.round(TILE_SIZE * p.zoom));
  const tileH = tileW;
  const colsVisible = Math.floor((w - MARGIN_X) / tileW) + 2;
  const rowsVisible = Math.floor((h - MARGIN_Y) / tileH) + 2;

  // map tiles
  for (let row = 0; row < rowsVisible; row++) {
    const mapRow = row + p.offsetY;
    if (mapRow < 0 || mapRow >= m.height) {
      continue;
    }
    for (let col = 0; col < colsVisible; col++) {
      const mapCol = col + p.offsetX;
      if (mapCol < 0 || mapCol >= m.width) {
        continue;
      }
      const tileID = m.tiles[mapRow]?.[mapCol];
      const px = MARGIN_X + col * tileW;
      const py = MARGIN_Y + row * tileH;
      if (tileID !== undefined && tileID < p.tiles.length && p.tiles[tileID]) {
        ctx.drawImage(p.tiles[tileID], px, py, tileW, tileH);
      } else {
        ctx.fillStyle = (col + row) % 2 === 0 ? "#505050" : "#3c3c3c";
        ctx.fillRect(px, py, tileW, tileH);
      }
    }
  }

  // NPCs
  if (p.showNpcOverlay) {
    m.npcs.forEach((npc, i) => {
      const { px, py } = tileToPixel(npc.x, npc.y, p.offsetX, p.offsetY, p.zoom);
      if (
        npc.x < p.offsetX ||
        npc.y < p.offsetY ||
        px >= w ||
        py >= h
      ) {
        return;
      }
      const sid = npc.sprite_id;
      if (sid < p.npcTiles.length && p.npcTiles[sid]) {
        ctx.drawImage(p.npcTiles[sid], px, py, tileW, tileH);
      }
      drawRect(ctx, px, py, tileW, tileH, i === p.selectedNpc ? "#00c8ff" : "#0064ff");
    });
  }

  // scripts
  if (p.showScriptOverlay) {
    m.scripts.forEach((sc, i) => {
      const { px, py } = tileToPixel(sc.x, sc.y, p.offsetX, p.offsetY, p.zoom);
      if (sc.x < p.offsetX || sc.y < p.offsetY || px >= w || py >= h) {
        return;
      }
      if (p.layer === "script") {
        // script layer: filled highlight + id number so scripts are easy to spot
        ctx.fillStyle = i === p.selectedScript ? "rgba(100,255,100,0.35)" : "rgba(0,200,80,0.28)";
        ctx.fillRect(px, py, tileW, tileH);
        drawRect(
          ctx,
          px,
          py,
          tileW,
          tileH,
          i === p.selectedScript ? "#64ff64" : "#00c850"
        );
        const labelScale = Math.max(1, Math.floor(tileW / 8));
        drawLabel(ctx, String(i), px + tileW / 2, py + tileH / 2, labelScale, "#ffffff");
      } else {
        drawRect(
          ctx,
          px,
          py,
          tileW,
          tileH,
          i === p.selectedScript ? "#64ff64" : "#00c850"
        );
      }
    });
  }

  // collision overlay
  if (p.showCollision && m.collision.length > 0) {
    for (let row = 0; row < rowsVisible; row++) {
      const mapRow = row + p.offsetY;
      if (mapRow < 0 || mapRow >= m.height) {
        continue;
      }
      for (let col = 0; col < colsVisible; col++) {
        const mapCol = col + p.offsetX;
        if (mapCol < 0 || mapCol >= m.width) {
          continue;
        }
        const cid = m.collision[mapRow]?.[mapCol];
        if (!cid) {
          continue;
        }
        let clr: RGBA = [255, 0, 0, 100];
        for (const ct of m.collision_types) {
          if (ct.id === cid) {
            clr = parseHexColor(ct.color);
            break;
          }
        }
        const px = MARGIN_X + col * tileW;
        const py = MARGIN_Y + row * tileH;
        fillTint(ctx, px, py, tileW, tileH, clr);
      }
    }
  }

  // encounter overlay
  if (p.showEncounters && m.encounters.length > 0) {
    for (let row = 0; row < rowsVisible; row++) {
      const mapRow = row + p.offsetY;
      if (mapRow < 0 || mapRow >= m.height) {
        continue;
      }
      for (let col = 0; col < colsVisible; col++) {
        const mapCol = col + p.offsetX;
        if (mapCol < 0 || mapCol >= m.width) {
          continue;
        }
        const zid = m.encounters[mapRow]?.[mapCol];
        if (zid === undefined || zid < 0) {
          continue;
        }
        const px = MARGIN_X + col * tileW;
        const py = MARGIN_Y + row * tileH;
        fillTint(ctx, px, py, tileW, tileH, zoneColor(zid));
      }
    }
  }

  // ghost block/paste preview
  const ghostTiles = p.tool === "paste" ? p.clipboard?.tiles ?? [] : p.activeBlock;
  const ghostW = p.tool === "paste" ? p.clipboard?.w ?? 0 : p.blockW;
  const ghostH = p.tool === "paste" ? p.clipboard?.h ?? 0 : p.blockH;
  if (ghost.visible && ghostTiles.length > 0) {
    for (let dr = 0; dr < ghostH; dr++) {
      for (let dc = 0; dc < ghostW; dc++) {
        const mapCol = ghost.col + dc;
        const mapRow = ghost.row + dr;
        if (mapCol < 0 || mapCol >= m.width || mapRow < 0 || mapRow >= m.height) {
          continue;
        }
        const { px, py } = tileToPixel(mapCol, mapRow, p.offsetX, p.offsetY, p.zoom);
        const tileID = ghostTiles[dr * ghostW + dc];
        if (tileID !== undefined && tileID < p.tiles.length && p.tiles[tileID]) {
          ctx.globalAlpha = 0.5;
          ctx.drawImage(p.tiles[tileID], px, py, tileW, tileH);
          ctx.globalAlpha = 1;
        }
      }
    }
  }

  // cell grid (clamped to map)
  ctx.strokeStyle = GRID_COLOR;
  ctx.lineWidth = 1;
  ctx.beginPath();
  const maxGridX = Math.min(w, MARGIN_X + (m.width - p.offsetX) * tileW);
  const maxGridY = Math.min(h, MARGIN_Y + (m.height - p.offsetY) * tileH);
  for (let col = 0; col <= colsVisible; col++) {
    if (col + p.offsetX > m.width) {
      break;
    }
    const x = MARGIN_X + col * tileW;
    if (x >= w) {
      break;
    }
    ctx.moveTo(x, MARGIN_Y);
    ctx.lineTo(x, maxGridY);
  }
  for (let row = 0; row <= rowsVisible; row++) {
    if (row + p.offsetY > m.height) {
      break;
    }
    const y = MARGIN_Y + row * tileH;
    if (y >= h) {
      break;
    }
    ctx.moveTo(MARGIN_X, y);
    ctx.lineTo(maxGridX, y);
  }
  ctx.stroke();

  const selection = ghost.selection ?? p.selection;
  if (selection) {
    const { px, py } = tileToPixel(selection.x, selection.y, p.offsetX, p.offsetY, p.zoom);
    const sw = selection.w * tileW;
    const sh = selection.h * tileH;
    ctx.fillStyle = "rgba(80, 190, 255, 0.16)";
    ctx.fillRect(px, py, sw, sh);
    ctx.strokeStyle = "#55d8ff";
    ctx.lineWidth = 2;
    ctx.strokeRect(px + 1, py + 1, sw - 2, sh - 2);
    ctx.fillStyle = "#d8f5ff";
    ctx.font = "12px system-ui";
    ctx.fillText(`${selection.x},${selection.y} ${selection.w}×${selection.h}`, px + 3, py + 14);
  }

  // chunk overlay
  if (p.showChunks) {
    ctx.strokeStyle = "rgba(255,255,0,0.31)";
    ctx.beginPath();
    for (let col = 0; col <= colsVisible; col++) {
      const mapCol = col + p.offsetX;
      if (mapCol > m.width) {
        break;
      }
      if (mapCol % CHUNK_W === 0) {
        const x = MARGIN_X + col * tileW;
        if (x >= w) {
          break;
        }
        ctx.moveTo(x, MARGIN_Y);
        ctx.lineTo(x, maxGridY);
      }
    }
    for (let row = 0; row <= rowsVisible; row++) {
      const mapRow = row + p.offsetY;
      if (mapRow > m.height) {
        break;
      }
      if (mapRow % CHUNK_H === 0) {
        const y = MARGIN_Y + row * tileH;
        if (y >= h) {
          break;
        }
        ctx.moveTo(MARGIN_X, y);
        ctx.lineTo(maxGridX, y);
      }
    }
    ctx.stroke();
  }

  // chunk highlight (shift-toggle H key)
  if (p.showChunkHighlight) {
    const chunksPerRow = Math.ceil(m.width / CHUNK_W);
    for (let row = 0; row < rowsVisible; row++) {
      const mapRow = row + p.offsetY;
      if (mapRow < 0 || mapRow >= m.height) {
        continue;
      }
      const chunkRow = Math.floor(mapRow / CHUNK_H);
      if (mapRow !== chunkRow * CHUNK_H) {
        continue;
      }
      for (let col = 0; col < colsVisible; col++) {
        const mapCol = col + p.offsetX;
        if (mapCol < 0 || mapCol >= m.width) {
          continue;
        }
        const chunkCol = Math.floor(mapCol / CHUNK_W);
        if (mapCol !== chunkCol * CHUNK_W) {
          continue;
        }
        // fill chunk rect
        const { px, py } = tileToPixel(
          chunkCol * CHUNK_W,
          chunkRow * CHUNK_H,
          p.offsetX,
          p.offsetY,
          p.zoom
        );
        const cw = Math.min(CHUNK_W, m.width - chunkCol * CHUNK_W);
        const ch = Math.min(CHUNK_H, m.height - chunkRow * CHUNK_H);
        fillTint(ctx, px, py, cw * tileW, ch * tileH, [200, 0, 0, 40]);
        const chunkID = chunkRow * chunksPerRow + chunkCol;
        const chunkPixH = ch * tileH;
        const chunkPixW = cw * tileW;
        const scaleH = Math.floor((chunkPixH * 4) / 5 / 5);
        const digits = String(chunkID);
        const scaleW = Math.floor((chunkPixW * 4) / 5 / (digits.length * 4 - 1));
        const scale = Math.max(1, Math.min(scaleH, scaleW));
        drawLabel(
          ctx,
          digits,
          px + (cw * tileW) / 2,
          py + (ch * tileH) / 2,
          scale,
          "rgba(255,80,80,0.86)"
        );
      }
    }
  }

  // coordinate labels
  let step = 1;
  while (tileW * step < 20) {
    step *= 2;
  }
  for (let col = 0; col < colsVisible; col++) {
    const mapCol = col + p.offsetX;
    if (mapCol >= m.width) {
      break;
    }
    if (mapCol % step !== 0) {
      continue;
    }
    drawLabel(
      ctx,
      String(mapCol),
      MARGIN_X + col * tileW + tileW / 2,
      MARGIN_Y / 2,
      1,
      "rgba(200,200,200,1)"
    );
  }
  for (let row = 0; row < rowsVisible; row++) {
    const mapRow = row + p.offsetY;
    if (mapRow >= m.height) {
      break;
    }
    if (mapRow % step !== 0) {
      continue;
    }
    drawLabel(
      ctx,
      String(mapRow),
      MARGIN_X / 2,
      MARGIN_Y + row * tileH + tileH / 2,
      1,
      "rgba(200,200,200,1)"
    );
  }
}

function drawRect(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  color: string
) {
  ctx.strokeStyle = color;
  ctx.lineWidth = 1;
  ctx.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
}

/** Blend a translucent color over existing pixels in a rect (Go blendOver parity). */
function fillTint(
  ctx: CanvasRenderingContext2D,
  x: number,
  y: number,
  w: number,
  h: number,
  clr: RGBA
) {
  if (clr[3] === 0) {
    return;
  }
  const img = ctx.getImageData(x, y, w, h);
  const d = img.data;
  for (let i = 0; i < d.length; i += 4) {
    const dst: RGBA = [d[i], d[i + 1], d[i + 2], d[i + 3]];
    const out = blendOver(dst, withAlpha(clr, clr[3]));
    d[i] = out[0];
    d[i + 1] = out[1];
    d[i + 2] = out[2];
  }
  ctx.putImageData(img, x, y);
}