use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Entire project as it comes out of the JSON loader.
///
/// We keep everything in very “raw” form so later stages
/// (assembler, compiler, etc.) can decide what they need.
#[derive(Debug)]
pub struct RawTiled {
    pub map: MapLayer,
    pub scripts: ScriptLayer,
    pub locations: LocationLayer,
}

/// ─────────────────────────────────────────────────────
/// Individual layer types
/// ─────────────────────────────────────────────────────
#[derive(Debug, Deserialize)]
pub struct MapLayer {
    #[serde(flatten)]
    pub raw: serde_json::Value,
}

/// A single "script object" coming from the Tiled layer.
/// Only the 3 fields required by the compiler/VM are kept.
#[derive(Debug, Clone)]
pub struct ScriptEntry {
    pub script: String,
    pub x: f32,
    pub y: f32,
}

/// Holds **all** objects that belong to Tiled's "script" layer.
#[derive(Debug, Clone)]
pub struct ScriptLayer {
    pub objects: Vec<ScriptEntry>,
}

#[derive(Debug, Deserialize)]
pub struct LocationLayer {
    #[serde(flatten)]
    pub raw: serde_json::Value,
}

/// Immediately-after-parse representation (raw, 1-to-1 with JSON).
#[derive(Debug, Clone)]
pub struct RawProject {
    pub scripts: ScriptLayer,
}

/// Fully processed output handed to `writer`.
/// (Will contain map, tiles, etc. later; right now only the VM part.)
use crate::processor::{ast::Cmd, vm};
pub struct ProcessedProject {
    pub vm: vm::ProcessedScripts,
}

#[derive(Debug, Clone)]
pub struct Script {
    pub body: Vec<Cmd>,
    pub x: i32,
    pub y: i32,
}

pub struct ParsedScripts {
    pub scripts: Vec<Script>,
    pub tags: HashMap<String, u16>,
    pub flags: HashMap<String, u16>,
}
