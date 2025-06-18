// numeric constants that describe the world / chunk grid
pub const MAP_W: i32 = 256;
pub const MAP_H: i32 = 256;

pub const CHUNK_W: i32 = 8;
pub const CHUNK_H: i32 = 4;

pub const CHUNK_COLS: i32 = MAP_W / CHUNK_W; // 32
pub const CHUNK_ROWS: i32 = MAP_H / CHUNK_H; // 64
pub const TOTAL_CHUNKS: usize = (CHUNK_COLS * CHUNK_ROWS) as usize;

use crate::processor::ast::Cmd;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Script {
    pub body: Vec<Cmd>,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone)]
pub struct ParsedScripts {
    /// Chunked representation; `chunks[idx]` holds all scripts that belong
    /// to that chunk – vector length is always `TOTAL_CHUNKS`.
    pub chunks: Vec<Vec<Script>>,

    pub tags: HashMap<String, u16>,
    pub flags: HashMap<String, u16>,
    pub texts: HashMap<String, u16>,
}

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
    pub id: i32,
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
use crate::processor::blob;
pub struct ProcessedProject {
    pub vm: blob::ProcessedScripts,
    pub flags: HashMap<String, u16>,
    pub locations: HashMap<String, u16>,
    pub texts: HashMap<String, u16>,
}
