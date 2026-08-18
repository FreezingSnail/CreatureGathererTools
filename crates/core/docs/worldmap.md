# CreatureGatherer Sprite Sheet Summary

> **Correction (2026-08-17):** Tile **0 is blank/void and must not be used.**
> The semantic IDs below (0-12) are *categories*, not raw tile IDs. Raw tile
> IDs verified programmatically: floor `6`/`7`, path `94`, wall core `274`,
> wall edges `257/259/273/275`, decor `16/29/37/527/62`.

This sprite sheet contains 768 tiles, organized into distinct categories and grouped by
semantic range. The palette used is as follows:
- Dark grey: `#000`
- Medium grey: `#555`
- Light grey: `#AAA`
- White: `#FFF`

### Tile Catalog

| ID | Description | Grid Pos |
|----|-------------|----------|
| 0  | Solid Floor  | 1-3, 8-11, 16-19, 24-27, 32-35, 40-43, 48-51, 56-59, 64-67, 72-75 |
| 1  | Pathway      | 4, 12, 20, 28, 36, 44, 52, 60, 68, 76 |
| 2  | Wall Core    | 5-7, 13-15, 21-23, 29-31, 37-39, 45-47, 53-55, 61-63, 69-71 |
| 3  | Wall Edge (Left) | 8-11, 16-19, 24-27, 32-35, 40-43, 48-51, 56-59, 64-67, 72-75 |
| 4  | Wall Edge (Right) | 9-12, 17-20, 25-28, 33-36, 41-44, 49-52, 57-60, 65-68, 73-76 |
| 5  | Blocky Decor | 10-11, 18-19, 26-27, 34-35, 42-43, 50-51, 58-59, 66-67, 74-75 |
| 6  | Checker Decor | 14-15, 22-23, 30-31, 38-39, 46-47, 54-55, 62-63, 70-71 |
| 7  | Stone/Gravel | 12-13, 20-21, 28-29, 36-37, 44-45, 52-53, 60-61, 68-69, 76 |
| 8  | Water        | 16-17, 24-25, 32-33, 40-41, 48-49, 56-57, 64-65, 72-73 |
| 9  | Grass        | 18-19, 26-27, 34-35, 42-43, 50-51, 58-59, 66-67, 74-75 |
| 10 | Tree         | 20-21, 28-29, 36-37, 44-45, 52-53, 60-61, 68-69, 76 |
| 11 | Building     | 22-23, 30-31, 38-39, 46-47, 54-55, 62-63, 70-71 |
| 12 | Object       | 24-25, 32-33, 40-41, 48-49, 56-57, 64-65, 72-73 |

### Map-Building Pick List
- **Border Tile ID:** 0 (Solid Floor)
- **Floor Tile ID:** 0 (Solid Floor)
- **Path Tile ID:** 1 (Pathway)
- **Decor Tile ID:** 5 (Blocky Decor), 6 (Checker Decor)

