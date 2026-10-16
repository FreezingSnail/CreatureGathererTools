// numeric constants that describe the world / chunk grid
pub const MAP_W: i32 = 256;
pub const MAP_H: i32 = 256;

pub const CHUNK_W: i32 = 8;
pub const CHUNK_H: i32 = 4;

pub const CHUNK_COLS: i32 = MAP_W / CHUNK_W; // 32
pub const CHUNK_ROWS: i32 = MAP_H / CHUNK_H; // 64
pub const TOTAL_CHUNKS: usize = (CHUNK_COLS * CHUNK_ROWS) as usize;

use crate::processor::ast::Cmd;
use crate::wfcmap::{Stamp, WfcProject};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Script {
    pub script: String,
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
pub type MapLayer = Vec<u16>;

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
#[derive(Debug, Clone)]
pub struct LocationEntry {
    pub id: i32,
    pub name: String,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct LocationLayer {
    pub objects: Vec<LocationEntry>,
}

/// Immediately-after-parse representation (raw, 1-to-1 with JSON).
#[derive(Debug, Clone)]
pub struct RawProject {
    pub map: MapLayer,
    pub scripts: ScriptLayer,
    pub locations: LocationLayer,
}

/// Fully processed output handed to `writer`.
/// (Will contain map, tiles, etc. later; right now only the VM part.)
use crate::processor::blob;
pub struct ProcessedProject {
    pub blob: blob::ProcessedScripts,
    pub flags: HashMap<String, u16>,
    pub locations: HashMap<String, u16>,
    pub texts: HashMap<String, u16>,
    pub map: Vec<MapLayer>,
    pub raw_map: MapLayer,
}

// ─────────────────────────────────────────────────────
// Canonical game-data model (ported from creatureTools
// internal/model/model.go). JSON field names mirror the
// Go struct tags exactly so existing data files keep
// working.
// ─────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Creature {
    pub name: String,
    pub id: i32,
    pub type1: String,
    pub type2: String,
    #[serde(rename = "evelevel")]
    pub evo_level: i32,
    pub atk: i32,
    #[serde(rename = "deff")]
    pub def: i32,
    #[serde(rename = "spcatk")]
    pub spc_atk: i32,
    #[serde(rename = "spcdef")]
    pub spc_def: i32,
    pub hp: i32,
    pub spd: i32,
    /// Known attack types (deduped authored names). Empty means no usable
    /// attack (WAIT). The generator folds these to the 5-bit duel mask.
    #[serde(default)]
    pub moves: Vec<String>,
}

/// Device-pinned five-type duel fold order; bit `i` of an attack mask is
/// `DUEL_FOLD_ORDER[i]`.
pub const DUEL_FOLD_ORDER: [&str; 5] = ["spirit", "water", "earth", "fire", "plant"];

/// Fold an authored 8-type name to its duel type index, or `None` for
/// ineligible types (`none`/`status`) and unknown names.
pub fn fold_type_name(name: &str) -> Option<u8> {
    let folded = match name {
        "spirit" | "elder" => "spirit",
        "water" | "wind" => "water",
        "earth" => "earth",
        "fire" | "lightning" => "fire",
        "plant" => "plant",
        _ => return None,
    };
    DUEL_FOLD_ORDER.iter().position(|t| *t == folded).map(|i| i as u8)
}

/// Fold a list of authored type names to a 5-bit attack mask (duplicates and
/// ineligible names are dropped).
pub fn attack_mask(names: &[String]) -> u8 {
    let mut mask = 0u8;
    for name in names {
        if let Some(bit) = fold_type_name(name) {
            mask |= 1u8 << bit;
        }
    }
    mask
}

/// One packed party slot in an opponent seed. `attack_mask` is the creature's
/// 5-bit duel attack set (bit i = duel type i).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpponentPartyMember {
    pub creature_id: u8,
    pub level: u8,
    pub attack_mask: u8,
}

/// Three-party opponent record consumed by the FX `OpponentSeed` reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpponentSeed {
    pub first: OpponentPartyMember,
    pub second: OpponentPartyMember,
    pub third: OpponentPartyMember,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NPCObject {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    #[serde(rename = "sprite_id")]
    pub sprite_id: i32,
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScriptObject {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollisionType {
    pub id: i32,
    pub name: String,
    pub color: String, // hex RGBA e.g. "#FF000080"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EncounterZone {
    pub id: i32,
    pub name: String,
    pub rate: i32, // 0-100
    #[serde(rename = "creature_ids")]
    pub creature_ids: Vec<i32>,
}

/// External Tiled tileset reference, retained in canonical maps so the
/// project loader can resolve tile properties without changing map GIDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileSetRef {
    pub firstgid: u32,
    pub source: String,
}

/// Our own canonical map format (schema we control).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameMap {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Vec<u16>>,
    /// External tilesets are metadata; `tiles` always retains original GIDs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tilesets: Vec<TileSetRef>,
    /// Resolved global GID -> six-bit tile-property mask. This is derived
    /// from TSX files by Store and never serialized into the canonical map.
    #[serde(skip)]
    pub tile_properties: BTreeMap<u16, u8>,
    /// Source path used for contextual build diagnostics. Not map data.
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
    pub npcs: Vec<NPCObject>,
    pub scripts: Vec<ScriptObject>,
    /// 0 = none, else CollisionType.ID
    ///
    /// `#[serde(default)]` matches Go's lenient `json.Unmarshal`: hand-authored
    /// maps (e.g. tools_dev_map.json) omit collision/encounter grids and load
    /// with zero values instead of erroring.
    #[serde(default)]
    pub collision: Vec<Vec<u8>>,
    /// user-defined types
    #[serde(default)]
    pub collision_types: Vec<CollisionType>,
    /// -1 = none, else EncounterZone.ID
    #[serde(default)]
    pub encounters: Vec<Vec<i32>>,
    #[serde(default)]
    pub encounter_zones: Vec<EncounterZone>,
    /// Optional WFC input project; omitted from ordinary maps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wfc: Option<WfcProject>,
    /// Tile-only stamp library, independent of WFC project data.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stamps: Vec<Stamp>,
}
