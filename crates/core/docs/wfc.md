# WFC hunk generation

CreatureGathererTools generates deterministic map hunks from author-defined tile pieces. Hunks are previewed in the WFC tab, optionally saved as RLE tile stamps, and pasted through the map editor into the 256×256 tile map. The saved stamp contains tiles, not a recipe reference, so later recipe edits cannot mutate an existing map.

## Layers

`crates/wfc` (`cgfx-wfc`) is a domain-free solver. It accepts a rectangular `Grid`, opaque `u32` symbols, directional `Rules`, weights, `Preset` domains, and a deterministic xoshiro256** RNG. It has no tile, map, JSON, or filesystem dependency. `solve_with_filter` adds an `ObservationFilter` hook; the default `solve` path remains unchanged. The crate is separate because `crates/core` is a behavior-identical port of the deprecated Go toolchain with parity tests, while WFC has no Go counterpart.

`crates/core/src/wfcmap` is the map-domain adapter. It loads `GameMap.wfc`, slices pieces, compiles sockets and placement rules, invokes `cgfx-wfc`, and bakes symbols into tile hunks. `src-tauri` exposes `capture_piece`, `wfc_generate`, and `wfc_validate`; the WFC tab authors pieces, recipes, anchors, rules, previews, and stamps.

## Slot model

The WFC project owns one slot size, defaulting to 2×2 tiles. Valid authoring sizes are 1×1, 2×2, and 4×4. The screen is 128×64 pixels, or 8×4 tiles at the 16×16 tile scale; one screen is also one engine chunk. A 4×4 slot therefore shows two slots across a screen, making the placement lattice visible at the player-facing scale. The 256-tile map dimension is divisible by 1, 2, 4, and 8, so slots tile the map without partial cells.

Recipes choose a piece subset, slot dimensions, seed, size, socket compatibility, anchors, and placement rules. The solver budget is `cells × symbols × 4 × 2` bytes, capped at 128 MiB by default. Symbols are counted per recipe; a full map at 2×2 slots has 16,384 cells.

## Pieces and sockets

A piece may be larger than one slot. The adapter slices it into slot-sized symbols in row-major order. Internal edges receive private socket IDs scoped to the piece, so sibling slices mate only with their own siblings and a variable-size piece reassembles exactly or not at all. A partial placement at a region edge needs no special case: propagation removes slices whose sibling cannot fit. The anchor slice (`ox = 0`, `oy = 0`) carries the piece weight; weighting every slice would bias a k-slot piece by `weight^k`.

Boundary strips are compared in shared-cell order: north and south left-to-right, east and west top-to-bottom. Sockets can be derived from tile strips, named symmetric labels, or named directed labels. Recipe `socket_compat` pairs explicitly loosen label matching. Derived strips are recomputed, never persisted.

## Anchors and placement rules

An anchor pins a piece at a hunk-local slot coordinate and is compiled to a solver preset. It is the reliable way to state “this exact piece here” and the fastest socket diagnostic. Overlapping or out-of-bounds anchors are rejected before solving.

`WfcRecipe.rules` is serde-defaulted, so older project files load with no rules. Its tagged JSON forms are:

```json
{"kind":"count","piece_id":12,"min":1,"max":4}
{"kind":"exclude","a":12,"b":17,"radius":1}
{"kind":"border","piece_ids":[12,17]}
{"kind":"band","piece_id":12,"from_row":2,"to_row":5}
```

Count is an anchor-placement count. `max` is applied by the generic observation filter; `min` is checked after a solve and causes a seeded retry. The legacy `WfcPiece.limit` is folded into the same effective count-max rule. Exclude uses Chebyshev distance in slots and filters a candidate against already-decided anchor placements. Border presets allow only the listed pieces on border cells. Band presets prevent the selected piece outside its inclusive row range. Unsatisfiable rules return a typed `RuleUnsatisfied` error after bounded retries; no rule can hang generation.

## Determinism and persistence

For fixed `(project, recipe, size, seed)`, generated hunk tiles are byte-identical. The RNG is vendored xoshiro256** so dependency upgrades do not change recorded seeds. Solver contradictions and post-solve count-min failures retry with `seed + k`; all attempts are bounded. The hunk reports its effective seed and attempt count.

`GameMap.wfc` stores pieces, socket labels, recipes, anchors, compatibility pairs, and rules. `GameMap.stamps` stores RLE-encoded tile blocks. RLE keeps pretty-printed map JSON compact and avoids one tile per line. A stamp origin records recipe ID, seed, and slot size for provenance, but tiles remain authoritative.

The CLI uses the same adapter as IPC:

```sh
cargo run -p cgfx-core --bin cgfx-tools -- \
  wfc generate --map path/to/map.json --recipe cave --seed 42 --size 8x4 \
  --out path/to/cave-hunk.json
```

`--dry-run` prints the slot grid and counts without writing. `--in-place --stamp-name NAME` appends one stamp to the map; mutation is never implicit. The UI keeps the six most recent generated hunks in an ephemeral tray. Selecting a tray item previews it and saving it writes a normal stamp; clearing the tray does not change project data.

## Known gaps

Generation is region-local and does not seam-match an existing map edge. Only the tile layer is generated; collision is not derived. Global path connectivity is not enforced. In-place generation with surrounding seam constraints remains future work.
