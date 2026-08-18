Examine the attached sprite sheet image(s) — /Users/connorfranc/code/CreatureGathererFX/images/tiles.png
(16x48 grid of 16px tiles, grayscale sheet used by the CreatureGatherer map editor).

Task: produce a tile catalog and write it to /Users/connorfranc/code/CreatureGathererTools/crates/core/docs/spritesheet_summary.md

Rules:
- Tile index = row*16 + col (row 0 = top of image). Tile 0 is at top-left.
- Document EVERY distinct tile kind by index: describe what it depicts (floor, wall, path, edge, texture, glyph...).
- Group by semantic range: e.g. "rows 0-3: fine texture X", "rows 8-15: wall patterns".
- Identify with concrete IDs: solid/plain floor candidate(s), path candidate(s), wall core + wall edge stripes (left/right), blocky/checker decor, stone/gravel, and any tile that reads as water, grass, tree, building, or object.
- Note the palette (greys: #000 #555 #AAA #FFF) and that 768 tiles exist with many duplicates.
- End with a "map-building pick list": recommended border tile id, floor tile id, path tile id, decor tile id(s).

Format: normal English, markdown, tile catalog as a table (ID | description | grid pos). Write the file, confirm it with a re-read.