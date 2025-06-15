//! Public façade for JSON → RawProject parsing.

use std::fs;

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

pub fn load(path: &str) -> Result<RawProject> {
    let json = fs::read_to_string(path)?;
    let tiled = load_from_json(&json)?;
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

    let mut map: Option<MapLayer> = None;
    let mut scripts: Option<ScriptLayer> = None;
    let mut locations: Option<LocationLayer> = None;

    for layer_val in layers {
        // We only need the name to decide where to deserialize.
        let name = layer_val
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| anyhow!("layer missing `name` field"))?;

        match name {
            "map" => {
                map = Some(serde_json::from_value(layer_val.clone())?);
            }
            "scripts" => {
                scripts = Some(parse_script_layer(layer_val)?);
            }
            "locations" => {
                locations = Some(serde_json::from_value(layer_val.clone())?);
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
/// Helper: parse the “script” layer into a strongly-typed struct.
fn parse_script_layer(layer: &Value) -> Result<ScriptLayer> {
    let obj_arr = layer
        .get("objects")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("`script` layer has no `objects` array"))?;

    let mut entries = Vec::<ScriptEntry>::with_capacity(obj_arr.len());

    for obj in obj_arr {
        let x = obj
            .get("x")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!("object missing `x`"))? as f32;

        let y = obj
            .get("y")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| anyhow!("object missing `y`"))? as f32;

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
                        (Some("script"), Some(val)) => Some(val.to_string()),
                        _ => None,
                    }
                })
            })
            .ok_or_else(|| anyhow!("object missing `script` property"))?;

        entries.push(ScriptEntry {
            script: script_value,
            x,
            y,
        });
    }

    Ok(ScriptLayer { objects: entries })
}

pub fn tiled_to_raw(tiled: &RawTiled) -> RawProject {
    RawProject {
        scripts: tiled.scripts.clone(),
    }
}
