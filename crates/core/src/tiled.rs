//! Tiled JSON import/export interchange (`CreatureGathererFX-179.6`).
//!
//! Tiled is an **import/export interchange format only** — the canonical
//! in-app format is [`crate::model::GameMap`]. This module:
//!
//! * [`import`] parses a Tiled map file (layers `map` / `scripts` /
//!   `locations`, tolerating singular `script` with a warning) into a
//!   [`GameMap`].
//! * [`export`] serializes a [`GameMap`] back into Tiled JSON with the same
//!   three layers, reversing [`import`].
//!
//! Field mapping (mirrors `creaturevm`'s `parser` module and the Go Builder
//! `tiled/tiled_parser.go`, which expects layer names `map`/`scripts`/
//! `locations`):
//!
//! | Tiled layer           | GameMap field                        |
//! |-----------------------|--------------------------------------|
//! | `map` (tilelayer)     | `tiles` (2D, `y`-major, tile IDs)    |
//! | `scripts` (objectgroup) | `scripts: Vec<ScriptObject>`       |
//! | `locations` (objectgroup) | `npcs: Vec<NPCObject>`           |
//!
//! Layering decisions (documented, per issue design):
//!
//! * **Locations → NPCs**: each Tiled location object `{id, name, x, y}` maps
//!   to an [`NPCObject`] with `sprite_id: 0`. `NPCObject` has **no `name`
//!   field**, so the location's `@name` (e.g. `@test_house`) is carried in the
//!   `script` field; `export` writes it back as the location object's `name`,
//!   making location export lossless. Tiled script objects are distinct and
//!   keep their text in `ScriptObject.script`.
//! * **Pixel → tile coordinates**: Tiled object `x`/`y` are pixel coords
//!   (16 px per tile, matching `locations_parser`'s `/16.0` and
//!   `script_parser`'s `/ 16` conventions, plus the Go `tiled_parser.go`
//!   fixtures). Import converts with `floor(px / 16)`; export writes
//!   `tile * 16`.
//! * **Default grids**: import has no collision/encounter data, so it
//!   synthesizes `collision` filled with `0` (none) and `encounters` filled
//!   with `-1` (none) so consumers can index them; `collision_types` and
//!   `encounter_zones` are empty.
//!
//! Unknown or missing layers are hard errors, mirroring
//! [`crate::parser::load_from_json`].

use std::error::Error;
use std::fmt;

use serde_json::{Value, json};

use crate::model::{GameMap, NPCObject, ScriptObject};

/// Tile size in pixels; Tiled object coordinates are pixel-based and the
/// canonical GameMap model stores tile coordinates.
pub const TILE_SIZE: f64 = 16.0;

/// Error produced by Tiled JSON import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiledError(pub String);

impl fmt::Display for TiledError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for TiledError {}

/// Parse Tiled JSON (`layers`: `map` / `scripts` / `locations`,
/// tolerating singular `script`) into the canonical [`GameMap`].
///
/// Missing or unknown layers are reported as errors, mirroring
/// [`crate::parser::load_from_json`].
pub fn import(json: &str) -> Result<GameMap, TiledError> {
    let root: Value =
        serde_json::from_str(json).map_err(|e| TiledError(format!("Failed to parse JSON: {e}")))?;

    let layers = root
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TiledError("file has no `layers` array".to_string()))?;

    let width = root
        .get("width")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| TiledError("missing `width`".to_string()))? as u32;
    let height = root
        .get("height")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| TiledError("missing `height`".to_string()))? as u32;

    let mut map_data: Option<Vec<u16>> = None;
    let mut scripts: Option<Vec<ScriptObject>> = None;
    let mut locations: Option<Vec<NPCObject>> = None;

    for (i, layer_val) in layers.iter().enumerate() {
        let name = layer_val
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| TiledError(format!("layer {i} missing `name` field")))?;

        match name {
            "map" => {
                map_data = Some(parse_map_layer(layer_val)?);
            }
            "script" => {
                eprintln!(
                    "warning: layer named `script` (singular) is non-canonical; treating it as `scripts`"
                );
                scripts = Some(parse_scripts_layer(layer_val)?);
            }
            "scripts" => {
                scripts = Some(parse_scripts_layer(layer_val)?);
            }
            "locations" => {
                locations = Some(parse_locations_layer(layer_val)?);
            }
            other => return Err(TiledError(format!("unknown layer `{other}`"))),
        }
    }

    let map_data = map_data.ok_or_else(|| TiledError("`map` layer missing".to_string()))?;
    let scripts = scripts.ok_or_else(|| TiledError("`script` layer missing".to_string()))?;
    let locations = locations.ok_or_else(|| TiledError("`locations` layer missing".to_string()))?;

    let expect = (width as usize).checked_mul(height as usize);
    let actual = map_data.len();
    match expect {
        Some(n) if n == actual => {}
        _ => {
            return Err(TiledError(format!(
                "map data length {actual} does not match {width}x{height}"
            )));
        }
    }

    // Flat width-major data -> 2D y-major tiles.
    let mut tiles = Vec::with_capacity(height as usize);
    for y in 0..height as usize {
        let row = &map_data[y * (width as usize)..(y + 1) * (width as usize)];
        tiles.push(row.to_vec());
    }

    // No collision/encounter data in Tiled: synthesize defaults so grids are
    // always indexable (0 = no collision, -1 = no encounter zone).
    let collision = vec![vec![0u8; width as usize]; height as usize];
    let encounters = vec![vec![-1i32; width as usize]; height as usize];

    Ok(GameMap {
        width,
        height,
        tiles,
        npcs: locations,
        scripts,
        collision,
        collision_types: Vec::new(),
        encounters,
        encounter_zones: Vec::new(),
    })
}

/// Serialize a [`GameMap`] into Tiled JSON (`layers`: `map` / `scripts` /
/// `locations`), reversing [`import`].
pub fn export(map: &GameMap) -> String {
    let w = map.width as usize;
    let h = map.height as usize;

    // Flat width-major tile data.
    let mut data = Vec::with_capacity(w * h);
    for row in &map.tiles {
        data.extend(row.iter().map(|t| Value::from(*t)));
    }

    let next_object_id = map
        .npcs
        .iter()
        .map(|o| o.id + 1)
        .chain(map.scripts.iter().map(|o| o.id + 1))
        .max()
        .unwrap_or(1);

    let script_objects: Vec<Value> = map
        .scripts
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": "",
                "point": true,
                "x": s.x as f64 * TILE_SIZE,
                "y": s.y as f64 * TILE_SIZE,
                "width": 0,
                "height": 0,
                "rotation": 0,
                "visible": true,
                "type": "",
                "properties": [
                    { "name": "script", "type": "string", "value": s.script }
                ]
            })
        })
        .collect();

    // Locations layer: NPCObject.script carries the location `@name`
    // (NPCObject has no name field); write it back as the object name so the
    // locations layer round-trips losslessly.
    let location_objects: Vec<Value> = map
        .npcs
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "name": n.script,
                "point": true,
                "x": n.x as f64 * TILE_SIZE,
                "y": n.y as f64 * TILE_SIZE,
                "width": 0,
                "height": 0,
                "rotation": 0,
                "visible": true,
                "type": ""
            })
        })
        .collect();

    let map_layer = json!({
        "id": 1,
        "name": "map",
        "type": "tilelayer",
        "width": w,
        "height": h,
        "x": 0,
        "y": 0,
        "opacity": 1,
        "visible": true,
        "data": data
    });

    let scripts_layer = json!({
        "id": 2,
        "name": "scripts",
        "type": "objectgroup",
        "draworder": "topdown",
        "x": 0,
        "y": 0,
        "opacity": 1,
        "visible": true,
        "objects": script_objects
    });

    let locations_layer = json!({
        "id": 3,
        "name": "locations",
        "type": "objectgroup",
        "draworder": "topdown",
        "x": 0,
        "y": 0,
        "opacity": 1,
        "visible": true,
        "objects": location_objects
    });

    let value = json!({
        "compressionlevel": -1,
        "height": h,
        "infinite": false,
        "layers": [map_layer, scripts_layer, locations_layer],
        "nextlayerid": 4,
        "nextobjectid": next_object_id,
        "orientation": "orthogonal",
        "renderorder": "right-down",
        "tiledversion": "1.10.2",
        "tileheight": TILE_SIZE as u32,
        "tilesets": [],
        "tilewidth": TILE_SIZE as u32,
        "type": "map",
        "version": "1.10",
        "width": w
    });

    serde_json::to_string(&value).expect("serializing a Value cannot fail")
}

/// Parse the `map` tilelayer's `data` array into flat tile IDs.
fn parse_map_layer(layer: &Value) -> Result<Vec<u16>, TiledError> {
    let data = layer
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TiledError("`map` layer has no `data` array".to_string()))?;

    let mut buf = Vec::with_capacity(data.len());
    for v in data {
        let id = v
            .as_u64()
            .ok_or_else(|| TiledError("invalid map data".to_string()))?;
        if id > u16::MAX as u64 {
            return Err(TiledError(format!("map tile id {id} exceeds u16")));
        }
        buf.push(id as u16);
    }
    Ok(buf)
}

/// Parse `scripts`/`script` objectgroup objects into [`ScriptObject`]s.
fn parse_scripts_layer(layer: &Value) -> Result<Vec<ScriptObject>, TiledError> {
    let obj_arr = layer
        .get("objects")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TiledError("`script` layer has no `objects` array".to_string()))?;

    let mut out = Vec::with_capacity(obj_arr.len());
    for (i, obj) in obj_arr.iter().enumerate() {
        let id = f64_to_i32(obj, "id", i)?;
        let x = tile_coord(obj, "x", i)?;
        let y = tile_coord(obj, "y", i)?;

        // Locate the property whose name == "script".
        let script_value = obj
            .get("properties")
            .and_then(|v| v.as_array())
            .and_then(|props| {
                props.iter().find_map(|p| {
                    match (
                        p.get("name").and_then(|n| n.as_str()),
                        p.get("value").and_then(|v| v.as_str()),
                    ) {
                        (Some("script"), Some(val)) => Some(val.to_string()),
                        _ => None,
                    }
                })
            })
            .ok_or_else(|| {
                TiledError(format!(
                    "object {i} at ({x}, {y}) missing `script` property"
                ))
            })?;

        if script_value.len() > 10_000 {
            eprintln!(
                "warning: script at ({x}, {y}) is very long: {} chars",
                script_value.len()
            );
        }

        out.push(ScriptObject {
            id,
            x,
            y,
            script: script_value,
        });
    }
    Ok(out)
}

/// Parse `locations` objectgroup objects into [`NPCObject`]s.
///
/// Mapping (see module docs): `{id, name, x, y}` → `NPCObject {id, x, y,
/// sprite_id: 0, script: name}` — the location `@name` is carried in the
/// `script` field because the canonical model has no NPC name field.
fn parse_locations_layer(layer: &Value) -> Result<Vec<NPCObject>, TiledError> {
    let obj_arr = layer
        .get("objects")
        .and_then(|v| v.as_array())
        .ok_or_else(|| TiledError("`locations` layer has no `objects` array".to_string()))?;

    let mut out = Vec::with_capacity(obj_arr.len());
    for (i, obj) in obj_arr.iter().enumerate() {
        let id = f64_to_i32(obj, "id", i)?;
        let x = tile_coord(obj, "x", i)?;
        let y = tile_coord(obj, "y", i)?;
        let name = obj
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| TiledError(format!("object {i} missing `name`")))?;

        out.push(NPCObject {
            id,
            x,
            y,
            sprite_id: 0,
            script: name.to_string(),
        });
    }
    Ok(out)
}

/// Object field as a raw number, floored to `i32` (mirrors Go
/// `math.Floor` in `tiled_parser.go`).
fn f64_to_i32(obj: &Value, field: &str, idx: usize) -> Result<i32, TiledError> {
    obj.get(field)
        .and_then(|v| v.as_f64())
        .map(|v| v.floor() as i32)
        .ok_or_else(|| TiledError(format!("object {idx} missing `{field}`")))
}

/// Pixel coordinate → tile coordinate (`floor(px / 16)`).
fn tile_coord(obj: &Value, field: &str, idx: usize) -> Result<i32, TiledError> {
    let px = obj
        .get(field)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| TiledError(format!("object {idx} missing `{field}`")))?;
    Ok((px / TILE_SIZE).floor() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata");

    fn read_fixture(name: &str) -> String {
        fs::read_to_string(format!("{FIXTURE_DIR}/{name}"))
            .unwrap_or_else(|e| panic!("read {name}: {e}"))
    }

    fn stub_map(width: u32, height: u32, len: usize) -> Value {
        json!({
            "width": width,
            "height": height,
            "layers": [
                { "name": "map", "type": "tilelayer", "width": width, "height": height,
                  "data": vec![0u16; len] },
                { "name": "scripts", "type": "objectgroup", "objects": [] },
                { "name": "locations", "type": "objectgroup", "objects": [] }
            ]
        })
    }

    /// `testdata/tiled.json` is a bare script objectgroup layer (Go Builder
    /// fixture). Wrap its `objects` into a full container to exercise the
    /// import mapping of its content.
    #[test]
    fn import_tiled_fixture_scripts_layer() {
        let fixture: Value = serde_json::from_str(&read_fixture("tiled.json")).unwrap();
        let objects = fixture.get("objects").unwrap().clone();

        let container = json!({
            "width": 16,
            "height": 16,
            "layers": [
                { "name": "map", "type": "tilelayer", "width": 16, "height": 16,
                  "data": vec![0u16; 16 * 16] },
                { "name": "scripts", "type": "objectgroup", "objects": objects },
                { "name": "locations", "type": "objectgroup", "objects": [] }
            ]
        });

        let map = import(&container.to_string()).unwrap();

        assert_eq!(map.width, 16);
        assert_eq!(map.height, 16);
        assert_eq!(map.tiles.len(), 16);
        assert_eq!(map.tiles[0].len(), 16);
        assert!(map.npcs.is_empty());

        // Pixel -> tile: floor(7.826 / 16) = 0, floor(75.346 / 16) = 4.
        assert_eq!(map.scripts.len(), 2);
        assert_eq!(map.scripts[0].id, 5);
        assert_eq!(map.scripts[0].x, 0);
        assert_eq!(map.scripts[0].y, 4);
        assert_eq!(map.scripts[0].script, "tp @test_teleport @test_house");
        assert_eq!(map.scripts[1].id, 6);
        assert_eq!(map.scripts[1].x, 8); // floor(135.686 / 16)
        assert_eq!(map.scripts[1].y, 4); // floor(75.018 / 16)
        assert_eq!(map.scripts[1].script, "tp @test_house @test_teleport");

        // Default grids synthesized.
        assert_eq!(map.collision, vec![vec![0u8; 16]; 16]);
        assert_eq!(map.encounters, vec![vec![-1i32; 16]; 16]);
    }

    /// Full real Tiled map fixture: 256x256 tile layer, 5 scripts, 2
    /// locations.
    #[test]
    fn import_world_map() {
        let map = import(&read_fixture("world_map.json")).unwrap();

        assert_eq!(map.width, 256);
        assert_eq!(map.height, 256);
        assert_eq!(map.tiles.len(), 256);
        assert_eq!(map.tiles[0].len(), 256);

        let flat: Vec<u16> = map
            .tiles
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect();
        assert_eq!(flat.len(), 65536);
        assert_eq!(flat[0], 7);
        assert_eq!(flat[65535], 1);
        assert!(flat.iter().all(|&t| t > 0 && t <= 298));

        // Locations -> NPCs: floor(px / 16).
        assert_eq!(map.npcs.len(), 2);
        assert_eq!(
            map.npcs[0],
            NPCObject {
                id: 7,
                x: 12,
                y: 7,
                sprite_id: 0,
                script: "@test_house".into()
            }
        );
        assert_eq!(
            map.npcs[1],
            NPCObject {
                id: 9,
                x: 4,
                y: 4,
                sprite_id: 0,
                script: "@test_teleport".into()
            }
        );

        // Scripts keep text from the `script` property.
        assert_eq!(map.scripts.len(), 5);
        assert_eq!(
            map.scripts[0],
            ScriptObject {
                id: 5,
                x: 0,
                y: 4,
                script: "tpif @test_teleport @test_house;".into()
            }
        );
        assert_eq!(
            map.scripts[1],
            ScriptObject {
                id: 6,
                x: 8,
                y: 4,
                script: "tpif @test_house @test_teleport;".into()
            }
        );
        assert_eq!(
            map.scripts[4],
            ScriptObject {
                id: 16,
                x: 2,
                y: 3,
                script: "tmsg 1 2 {who art thee};".into()
            }
        );
        assert!(map.collision_types.is_empty());
        assert!(map.encounter_zones.is_empty());
    }

    /// export(import(x)) reimports to an equivalent GameMap.
    #[test]
    fn export_roundtrips_world_map() {
        let original = import(&read_fixture("world_map.json")).unwrap();
        let exported = export(&original);

        // Export output is valid Tiled: layers named map/scripts/locations.
        let root: Value = serde_json::from_str(&exported).unwrap();
        let names: Vec<&str> = root["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["map", "scripts", "locations"]);
        assert_eq!(root["width"], 256);
        assert_eq!(root["height"], 256);

        let reimported = import(&exported).unwrap();
        assert_eq!(reimported, original);
    }

    /// Round-trip also works for a minimal map with no objects.
    #[test]
    fn export_roundtrips_empty() {
        let original = import(&stub_map(3, 2, 6).to_string()).unwrap();
        let reimported = import(&export(&original)).unwrap();
        assert_eq!(reimported, original);
    }

    /// Missing layers / missing `layers` array error clearly.
    #[test]
    fn missing_layers_error() {
        let err = import(r#"{"width":16,"height":16}"#).unwrap_err();
        assert!(err.0.contains("no `layers` array"), "got: {err}");

        let only_map = json!({
            "width": 2, "height": 2,
            "layers": [{ "name": "map", "type": "tilelayer",
                         "width": 2, "height": 2, "data": [0,0,0,0] }]
        });
        let err = import(&only_map.to_string()).unwrap_err();
        assert!(err.0.contains("`script` layer missing"), "got: {err}");

        let no_locations = json!({
            "width": 2, "height": 2,
            "layers": [
                { "name": "map", "type": "tilelayer", "width": 2, "height": 2,
                  "data": [0,0,0,0] },
                { "name": "scripts", "type": "objectgroup", "objects": [] }
            ]
        });
        let err = import(&no_locations.to_string()).unwrap_err();
        assert!(err.0.contains("`locations` layer missing"), "got: {err}");
    }

    /// Unknown layer names error clearly.
    #[test]
    fn unknown_layer_errors() {
        let bad = json!({
            "width": 2, "height": 2,
            "layers": [{ "name": "bogus", "type": "objectgroup", "objects": [] }]
        });
        let err = import(&bad.to_string()).unwrap_err();
        assert!(err.0.contains("unknown layer `bogus`"), "got: {err}");
    }

    /// Non-canonical singular `script` layer is tolerated (with a warning).
    #[test]
    fn singular_script_layer_tolerated() {
        let mut root: Value = serde_json::from_str(&read_fixture("world_map.json")).unwrap();
        for layer in root["layers"].as_array_mut().unwrap() {
            if layer["name"] == "scripts" {
                layer["name"] = Value::from("script");
            }
        }
        let map = import(&root.to_string()).unwrap();
        assert_eq!(map.scripts.len(), 5);
    }

    /// Tile IDs beyond u16 are rejected rather than silently truncated.
    #[test]
    fn oversized_tile_id_errors() {
        let bad = json!({
            "width": 1, "height": 1,
            "layers": [{ "name": "map", "type": "tilelayer",
                         "width": 1, "height": 1, "data": [70000] }]
        });
        let err = import(&bad.to_string()).unwrap_err();
        assert!(err.0.contains("exceeds u16"), "got: {err}");
    }
}
