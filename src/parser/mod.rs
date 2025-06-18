use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::model::{LocationLayer, MapLayer, RawProject, RawTiled, ScriptEntry, ScriptLayer};

/// Parse the whole input JSON string into `RawProject`.
///
/// The Tiled file is expected to contain a top-level `layers` array with
/// exactly three entries whose `name` property equals one of
///   • "map"
///   • "script"
///   • "locations"
///
/// Any additional layer or a missing one is reported as an error.
pub fn load(json: &str) -> Result<RawProject> {
    println!("File loaded, size: {} bytes", json.len());
    let tiled = load_from_json(&json).map_err(|e| anyhow!("Failed to parse JSON: {}", e))?;
    println!("JSON parsed successfully");

    let raw = tiled_to_raw(&tiled);
    Ok(raw)
}

pub fn load_from_json(json: &str) -> Result<RawTiled> {
    // Grab the entire file as a dynamic value first.
    let root: Value = serde_json::from_str(json)?;

    let layers = root
        .get("layers")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("file has no `layers` array"))?;

    println!("Found {} layers", layers.len());

    let mut map: Option<MapLayer> = None;
    let mut scripts: Option<ScriptLayer> = None;
    let mut locations: Option<LocationLayer> = None;

    for (i, layer_val) in layers.iter().enumerate() {
        // We only need the name to decide where to deserialize.
        let name = layer_val
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow!("layer {} missing `name` field", i))?;

        println!("Processing layer: {}", name);

        match name {
            "map" => {
                map = Some(serde_json::from_value(layer_val.clone())?);
                println!("Map layer parsed");
            }
            "scripts" => {
                scripts = Some(parse_script_layer(layer_val)?);
                println!("Scripts layer parsed");
            }
            "locations" => {
                locations = Some(serde_json::from_value(layer_val.clone())?);
                println!("Locations layer parsed");
            }
            other => return Err(anyhow!("unknown layer `{other}`")),
        }
    }

    let map = map.ok_or_else(|| anyhow!("`map` layer missing"))?;
    let scripts = scripts.ok_or_else(|| anyhow!("`script` layer missing"))?;
    let locations = locations.ok_or_else(|| anyhow!("`locations` layer missing"))?;

    Ok(RawTiled {
        map,
        scripts,
        locations,
    })
}

// ─────────────────────────────────────────────────────
/// Helper: parse the "script" layer into a strongly-typed struct.
fn parse_script_layer(layer: &Value) -> Result<ScriptLayer> {
    let obj_arr = layer
        .get("objects")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("`script` layer has no `objects` array"))?;

    println!("Found {} script objects", obj_arr.len());

    let mut entries = Vec::<ScriptEntry>::with_capacity(obj_arr.len());

    for (i, obj) in obj_arr.iter().enumerate() {
        if i % 100 == 0 {
            println!("Processing script object {}/{}", i, obj_arr.len());
        }

        let x = obj
            .get("x")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!("object {} missing `x`", i))? as f32;

        let y = obj
            .get("y")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!("object {} missing `y`", i))? as f32;

        let id = obj
            .get("id")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!("object {} missing `id`", i))? as i32;

        // Locate the property whose name == "script"
        let script_value = obj
            .get("properties")
            .and_then(|v| v.as_array())
            .and_then(|props| {
                props.iter().find_map(|p| {
                    match (
                        p.get("name").and_then(|n| n.as_str()),
                        p.get("value").and_then(|v| v.as_str()),
                    ) {
                        (Some("script"), Some(val)) => {
                            // Limit script length to prevent system issues
                            if val.len() > 10000 {
                                println!(
                                    "Warning: script at ({}, {}) is very long: {} chars",
                                    x,
                                    y,
                                    val.len()
                                );
                            }
                            Some(val.to_string())
                        }
                        _ => None,
                    }
                })
            })
            .ok_or_else(|| anyhow!("object {} at ({}, {}) missing `script` property", i, x, y))?;

        entries.push(ScriptEntry {
            id,
            script: script_value,
            x,
            y,
        });
    }

    println!("Successfully parsed {} script entries", entries.len());
    Ok(ScriptLayer { objects: entries })
}

pub fn tiled_to_raw(tiled: &RawTiled) -> RawProject {
    RawProject {
        scripts: tiled.scripts.clone(),
    }
}
