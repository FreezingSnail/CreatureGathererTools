# CreatureGathererTools

Authoring toolchain for **CreatureGathererFX**, a GBA creature-battler (pixel arthouse game). Turns human-authored game content into engine-ready artifacts: C headers and a **PokerVM** binary bytecode blob (run by a VM from SPI-flash).

![Map editor](screenshots/map-editor.png)

## Features

- **Desktop UI** (Tauri 2 + React + Tailwind v4 + shadcn/ui) with four tabbed views — Map, Creature, Move, Layout — that stay mounted to preserve per-view state.
- **Map editor**: tile painting, layers, palette with resizable sprite sheet + pinch zoom, pan/paint tools, gesture pan/pinch zoom, double-click script edit.
- **Script pane**: live parse feedback (line/col errors, opcode preview) for the in-game script language (branches, counters, locations, commands).
- **Layout view**: graphical FX memory map of everything being packed — 256-byte page strips per section (canvas, colored by group, hover for name/hex address/length/kind), namespace size bars with byte shares, alignment padding surfaced as its own quantity, and a sortable/filterable field table with page spans and image `WxH×frames`. Data and save sections are drawn as separate address spaces. Needs the builder `.bin` files, so run **Compile all** first.
- **Deterministic compilation** of maps, creatures, and moves into C headers + binary blobs, byte-parity with the reference Go implementation (integration tests assert zero discrepancy).
- **Tiled interoperability**: import/export maps via Tiled-style JSON (`crates/core/src/tiled.rs`).
- **Offline, single-machine workflow**: no network services; game data lives in a sibling `CreatureGathererFX` checkout wired in via `src-tauri/creature-tools.json`.

## Architecture

Three layers, dependency direction `ui → src-tauri → crates/core`:

### 1. React frontend — `ui/`

React 18 + TypeScript + Vite + Tailwind v4 + shadcn/ui (Radix) primitives. Talks to the backend exclusively via `@tauri-apps/api` `invoke()` — never touches the filesystem directly. Pure logic extracted for unit testing: `state.ts`, `geometry.ts`, `tileset.ts`, `layoutModel.ts`, `lib/validation.ts`.

### 2. Tauri 2 shell — `src-tauri/` (crate `cgfx-app`, lib `cgfx_app_lib`)

Thin IPC layer over the core library. Every `#[tauri::command]` returns a serializable value or `Result<_, String>` — no panics across IPC, no `clap` here. Commands wrap `creaturevm` (cgfx-core): `load_config`/`save_config`, `load_map`/`save_map`, `load_creatures`/`save_creatures`, `load_moves`/`save_moves`, `compile_map`, `compile_all`, `layout_report`, `parse_script`, `read_file_bytes`, `tiled_import_path`, `tiled_export_path`.

`layout_report` returns the packer's layout metadata (fields with `addr`, `len`, `span`, kind, namespace, source, plus section offsets) without writing any artifact. It locates `fxlayout.toml` from the optional `fx_layout` config key, falling back to walking up from `output_dir` — never from the process working directory. Missing builder `.bin` files and a missing layout spec return distinct errors with their own remedies.

### 3. Rust core — `crates/core/` (lib `creaturevm`, bin `pokervm-rust`)

CLI + reusable library. Pipeline stages (see `src/main.rs`):

1. **Parse** (`parser/`) — load project JSON (creatures, moves, map).
2. **Process** (`processor/`) — lex/parse/assemble the script language to an AST (`ast.rs`) then to bytecode; `blob.rs` packs the binary.
3. **Write** (`writer/`) — emit C headers (`c.rs`) and the binary artifact (`bin.rs`, 24-bit little-endian via `u24.rs`). Binary layout: text block (offset table + ASCII blob) + bytecode block, padded to 256-byte alignment.

Supporting modules: `builder.rs` (map/creature/moves builders incl. worldmap chunking), `store.rs`, `config.rs`, `tiled.rs`, `model.rs` (domain types: `Creature`, `GameMap`, `Move`). See `crates/core/docs/` for `binary_format.md` and `worldmap.md`.

FX packing lives in `fxpack/`: `spec.rs` (`fxlayout.toml`), `layout.rs` (address assignment, alignment, save section), `carray.rs`/`symbol.rs` (source decoders), `sprite.rs`/`font.rs`/`image.rs`, `header.rs` (`fxdata.h`), and `pack.rs`. `pack::plan()` computes the field list and offsets with no filesystem writes; `pack::pack()` adds the writing. Each field carries its source byte count and its span to the next field, so alignment padding is a separate, reportable quantity rather than being folded into entry sizes.

`Config` (`src-tauri/creature-tools.json`) takes an optional `fx_layout` path pointing at the game checkout's `fxlayout.toml`. When omitted it is derived by walking up from `output_dir`, so existing configs keep working.

## Repo Layout

```
crates/core/        Rust library + CLI (cgfx-core / creaturevm)
crates/core/tests/  Integration tests incl. Go-parity suite
src-tauri/          Tauri 2 shell (IPC commands)
ui/                 React frontend
screenshots/        App screenshots
```

## Build & Test

```bash
# Rust core + shell
cargo build                                   # workspace: crates/core + src-tauri
cargo test                                    # unit + integration tests
cargo run -p cgfx-core --bin cgfx-tools -- <input.json> <output_dir>  # CLI only, no UI
cargo run -p cgfx-core --bin cgfx-tools -- --version                 # version stamp

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

# Full desktop app (from ui/; starts vite via beforeDevCommand)
npx tauri dev                                 # requires Rust toolchain
npx tauri build                               # packaged release build
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
