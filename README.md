# CreatureGathererTools

Authoring toolchain for **CreatureGathererFX**, an Arduboy FX demake and creature-collecting game. Turns human-authored game content into engine-ready artifacts: C headers and a **PokerVM** binary bytecode blob (run by a VM from SPI-flash).

![Map editor](screenshots/map-editor.png)

## Features

- **Desktop UI** (Tauri 2 + React + Tailwind v4 + shadcn/ui) with six tabbed views — Map, Blocks, Creature, Move, Layout, WFC — that stay mounted to preserve per-view state.
- **Map editor**: tile painting, layers, palette with resizable sprite sheet + pinch zoom, pan/paint tools, gesture pan/pinch zoom, double-click script edit, project blocks as paste brushes.
- **Block library**: paint reusable tile blocks from the project sprite sheet in the Blocks tab, save them in a project-level `blocks.json`, then use them as a map-editor brush or import them as WFC pieces. Map selections can be captured as blocks directly from the editor.
- **WFC hunk authoring**: import blocks (or per-tile fill pieces) as slot-aligned WFC pieces, assign derived or named sockets, build recipes with anchors and count/exclude/border/band rules, preview/reroll deterministic hunks, compare the six-entry hunk history tray, and save selected hunks as map stamps.
- **Script pane**: live parse feedback (line/col errors, opcode preview) for the in-game script language (branches, counters, locations, commands).
- **Layout view**: graphical FX memory map of everything being packed — 256-byte page strips per section (canvas, colored by group, hover for name/hex address/length/kind), namespace size bars with byte shares, alignment padding surfaced as its own quantity, and a sortable/filterable field table with page spans and image `WxH×frames`. Data and save sections are drawn as separate address spaces. Needs the builder `.bin` files, so run **Compile all** first.
- **Deterministic compilation** of maps, creatures, and moves into C headers + binary blobs, byte-parity with the reference Go implementation (integration tests assert zero discrepancy).
- **Tiled interoperability**: import/export maps via Tiled-style JSON (`crates/core/src/tiled.rs`).
- **Offline, single-machine workflow**: no network services; game data lives in a sibling `CreatureGathererFX` checkout selected at startup (a project folder containing `creature-tools.json`, or a manifest JSON chosen directly with the native file picker).

## Architecture

Four layers, dependency direction `ui → src-tauri → crates/core → crates/wfc`:

### 1. React frontend — `ui/`

React 18 + TypeScript + Vite + Tailwind v4 + shadcn/ui (Radix) primitives. Talks to the backend exclusively via `@tauri-apps/api` `invoke()` — never touches the filesystem directly. The Blocks tab authors the project block library; the map editor consumes blocks as paste brushes and can capture map selections as blocks; the WFC tab imports blocks as pieces and authors project-level slots, sockets, recipes and anchors, generating preview hunks it can save as map stamps. Pure logic extracted for unit testing: `state.ts`, `geometry.ts`, `tileset.ts`, `layoutModel.ts`, `tileGrid.ts`, `blocks.ts`, `wfcImport.ts`, `wfc.ts`, `lib/validation.ts`.

### 2. Tauri 2 shell — `src-tauri/` (crate `cgfx-app`, lib `cgfx_app_lib`)

Thin IPC layer over the core library. Every `#[tauri::command]` returns a serializable value or `Result<_, String>` — no panics across IPC, no `clap` here. Commands wrap `creaturevm` (cgfx-core): `load_config`/`load_project_config`/`save_config`, `load_map`/`save_map`, `load_blocks`/`save_blocks`, `load_creatures`/`save_creatures`, `load_moves`/`save_moves`, `compile_map`, `compile_all`, `layout_report`, `parse_script`, `read_file_bytes`, `capture_piece`, `wfc_generate`, `wfc_validate`, `tiled_import_path(path, current)`, `tiled_export_path`.

`load_blocks`/`save_blocks` read and write the project block library (`crates/core/src/blocks.rs`). Its path comes from the optional manifest key `blocks_json`, defaulting to `blocks.json` beside the manifest; a project with no blocks file yet loads as an empty library, and saves create missing parent directories. Blocks live outside the map on purpose: one library serves every map, the map editor brush, and WFC piece import.

`capture_piece` returns a slot-aligned `WfcPiece`; `wfc_generate` returns an in-memory hunk; `wfc_validate` returns validation messages. Tiled has no WFC or stamp representation, so `tiled_import_path` accepts the open map and preserves its `wfc` project and `stamps` while importing Tiled data.

`layout_report` returns the packer's layout metadata (fields with `addr`, `len`, `span`, kind, namespace, source, plus section offsets) without writing any artifact. It locates `fxlayout.toml` from the optional `fx_layout` config key, falling back to walking up from `output_dir` — never from the process working directory. Missing builder `.bin` files and a missing layout spec return distinct errors with their own remedies.

### 3. Rust core — `crates/core/` (lib `creaturevm`, bin `cgfx-tools`)

CLI + reusable library. Pipeline stages (see `src/main.rs`):

1. **Parse** (`parser/`) — load project JSON (creatures, moves, map); the project block library is read separately through `store.rs` (`blocks.rs`).
2. **Process** (`processor/`) — lex/parse/assemble the script language to an AST (`ast.rs`) then to bytecode; `blob.rs` packs the binary.
3. **Write** (`writer/`) — emit C headers (`c.rs`) and the binary artifact (`bin.rs`, 24-bit little-endian via `u24.rs`). Binary layout: text block (offset table + ASCII blob) + bytecode block, padded to 256-byte alignment.

`wfcmap/` is the map-domain adapter: it persists `GameMap.wfc` (project-level slot size, pieces, socket labels, recipes, anchors, compatibility pairs and serde-default placement rules) and `GameMap.stamps` (RLE tile blocks), slices pieces into solver symbols, compiles socket rules and anchors, then bakes a solved hunk. See [WFC hunk generation](crates/core/docs/wfc.md) for its schema, limits and CLI; see `crates/core/docs/` for `binary_format.md` and `worldmap.md`.

FX packing lives in `fxpack/`: `spec.rs` (`fxlayout.toml`), `layout.rs` (address assignment, alignment, save section), `carray.rs`/`symbol.rs` (source decoders), `sprite.rs`/`font.rs`/`image.rs`, `header.rs` (`fxdata.h`), and `pack.rs`. `pack::plan()` computes the field list and offsets with no filesystem writes; `pack::pack()` adds the writing. Each field carries its source byte count and its span to the next field, so alignment padding is a separate, reportable quantity rather than being folded into entry sizes.

`Config` is a project manifest: `creature-tools.json` in the selected project folder, or any JSON manifest chosen at startup. It takes an optional `fx_layout` path pointing at the game checkout's `fxlayout.toml`. When omitted it is derived by walking up from `output_dir`, so existing configs keep working.

### 4. Domain-free solver — `crates/wfc/` (crate `cgfx-wfc`)

The dependency-free solver accepts only opaque symbols, adjacency rules, weights, presets and a grid. It owns deterministic xoshiro256** RNG, restart, propagation-step and memory limits; it has no map, tile, JSON or filesystem dependency. WFC is separate because it has no counterpart in the deprecated Go toolchain whose behavior `crates/core` continues to parity-test.

## Repo Layout

```
crates/core/        Rust library + CLI (cgfx-core / creaturevm)
crates/core/tests/  Integration tests incl. Go-parity suite
crates/wfc/         Domain-free deterministic WFC solver (cgfx-wfc)
src-tauri/          Tauri 2 shell (IPC commands)
ui/                 React frontend
screenshots/        App screenshots
install.sh          Local release install (app + CLI)
```

## Build & Test

```bash
# Rust core + shell
cargo build                                   # workspace: crates/core + crates/wfc + src-tauri
cargo test                                    # unit + integration tests
cargo run -p cgfx-core --bin cgfx-tools -- <input.json> <output_dir>  # CLI only, no UI
cargo run -p cgfx-core --bin cgfx-tools -- --version                 # version stamp

# Generate a WFC hunk from map.wfc; --out writes standalone hunk JSON.
cargo run -p cgfx-core --bin cgfx-tools -- \
  wfc generate --map path/to/map.json --recipe cave --seed 42 --size 8x4 \
  --out path/to/cave-hunk.json

# Release cgfx-tools (tag must match the Cargo package version, prefixed with v)
git tag v0.1.0 && git push origin v0.1.0
# GitHub Release assets: cgfx-tools-macos-arm64, cgfx-tools-linux-x64,
# plus <asset>.sha256 for each asset. `cgfx-tools --version` prints
# `cgfx-tools 0.1.0`; the numeric version matches the release tag.

# React frontend (from ui/)
npm install
npm run dev                                   # vite dev server (UI only)
npm test                                      # vitest run
npm run build                                 # typecheck (tsc) + vite build

# Full desktop app (from the repo root; starts vite via beforeDevCommand)
ui/node_modules/.bin/tauri dev                # requires Rust toolchain
ui/node_modules/.bin/tauri build              # packaged release build

# Local install: release-build the app + CLI into the install prefix
./install.sh                                  # CGFX_INSTALL_PREFIX overrides the destination
```

Test notes:

- Frontend tests are **vitest + Testing Library**, co-located as `*.test.ts(x)` next to the source they cover.
- Backend tests live in `crates/core/tests/`; integration tests assert parity with the Go reference implementation.
- Rate-limiting/asset paths in `creature-tools.json` are machine-specific — do not commit changes to it.

## Task Tracking

Issues are tracked with **bd (beads)**:

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd close <id>         # Complete work
```

## Conventions & Patterns

- **Rust**: edition 2024, `anyhow` for error context, `serde` derive for models, errors cross IPC only as strings. Mixed-case module names (e.g. `tiled.rs`) are intentional.
- **TypeScript**: strict; UI components never do I/O directly — go through `invoke`. Keep geometry/canvas math in pure modules so it can be unit-tested without a DOM.
- **Tests are permanent** — co-located with source, never in temp directories, never deleted.

## Sprite source config

`cgfx-tools --sprite-config fxsprites.toml` accepts optional `shades` values `2`, `3`, or `4` for sprite and string image planes. Put the top-level setting before any TOML table header (especially before `[[set]]`):

```toml
shades = 2

[[set]]
output = "Sprites.h"
images = ["sprites/player_16x16.png"]
```

Top-level `shades` applies to sets and strings; a set's or `[strings]` block's explicit value overrides it. `shades = 2` emits B&W (1bpp) sprites.

## B&W sprite placeholders

Use `cgfx-tools --bw-placeholders <sprite-config.toml> --archive-dir <dir>` to rewrite every PNG referenced by `[[set]].images` and `[strings].font` as RGBA8 black/white placeholders. The red channel uses the device threshold (`R >= 128` becomes white; otherwise black); RGB changes, alpha and dimensions do not. `--archive-dir` is required.

Before any rewrite, the tool archives originals byte-for-byte at mirrored config-relative paths. Existing archive files are never overwritten: identical bytes are reused, but a different byte sequence aborts before any PNG changes. Archive writes are SHA-256 verified; successful runs print `bw-placeholders: archived <n>, converted <n>.` Rerunning is safe and idempotent.

To restore source art, point a sprite config at the archive copies and set `shades = 4`, for example:

```toml
shades = 4

[[set]]
output = "Sprites.h"
images = ["archive/sprites/player_16x16.png"]
```

Use the archived originals themselves for `[strings].font` or additional sets; never restore from the converted source PNGs.
