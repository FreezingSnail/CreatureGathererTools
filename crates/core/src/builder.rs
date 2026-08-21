//! Game-data builders, ported from creatureTools internal/builder
//! (creatures.go, moves.go). Emits the same `.bin` / `.hpp` artifacts
//! byte-for-byte as the Go builders; Go output is the reference contract.
//!
//! - `creatureData.bin`  : 14 bytes per creature, little-endian-ish raw
//!   bytes (each field truncated to one byte, in model order).
//! - `creatureData.hpp`  : `namespace CreatureData { uint8_t creatureData[] = {...}; }`
//! - `moves.bin`         : 4 bytes per move: packed u16 (big-endian) +
//!   effect byte + 0xFF effect2 byte.
//! - `moves.hpp`         : per-move name strings + `moveNames` pointer table.

use crate::config::Config;
use crate::model::{Creature, GameMap, Move};
use crate::script;
use crate::script::ToBytecode;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Builder failure with a human-readable message (mirrors Go's `error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError(pub String);

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BuildError {}

fn io_err(action: &str, path: &Path, e: std::io::Error) -> BuildError {
    BuildError(format!("{action} {}: {e}", path.display()))
}

/// Go `creatures.go` `creatureTypeIndex`: type name -> byte.
const CREATURE_TYPE_INDEX: [(&str, u8); 9] = [
    ("spirit", 0),
    ("water", 1),
    ("wind", 2),
    ("earth", 3),
    ("fire", 4),
    ("lightning", 5),
    ("plant", 6),
    ("elder", 7),
    ("none", 8),
];

fn creature_type_byte(ty: &str) -> Option<u8> {
    CREATURE_TYPE_INDEX
        .iter()
        .find(|(name, _)| *name == ty)
        .map(|(_, b)| *b)
}

/// Go `moves.go` `moveTypeIndex`: type name -> u16.
const MOVE_TYPE_INDEX: [(&str, u16); 9] = [
    ("spirit", 0),
    ("water", 1),
    ("wind", 2),
    ("earth", 3),
    ("fire", 4),
    ("lightning", 5),
    ("plant", 6),
    ("elder", 7),
    ("status", 8),
    // Go also maps "none" -> 15.
];

const MOVE_TYPE_NONE: u16 = 15;

fn move_type_index(ty: &str) -> Option<u16> {
    MOVE_TYPE_INDEX
        .iter()
        .find(|(name, _)| *name == ty)
        .map(|(_, v)| *v)
        .or_else(|| (ty == "none").then_some(MOVE_TYPE_NONE))
}

/// Go `moves.go` `effectIndex`: effect name -> byte (NONE -> 0xFF).
const EFFECT_INDEX: [(&str, u8); 31] = [
    ("NONE", 0xFF),
    ("DPRSD", 0),
    ("SOAKED", 1),
    ("BUFTD", 2),
    ("SOILED", 3),
    ("SCRCHD", 4),
    ("ZAPPED", 5),
    ("TANGLD", 6),
    ("REDCD", 7),
    ("ENLTND", 8),
    ("DRNCHD", 9),
    ("AIRSWPT", 10),
    ("GRNDED", 11),
    ("KINDLD", 12),
    ("CHRGD", 13),
    ("ENRCHD", 14),
    ("EVOLVD", 15),
    ("ATKDWN", 16),
    ("DEFDWN", 17),
    ("SPCADWN", 18),
    ("SPCDDWN", 19),
    ("SPDDWN", 20),
    ("ATKUP", 21),
    ("DEFUP", 22),
    ("SPCAUP", 23),
    ("SPCDUP", 24),
    ("SPDUP", 25),
    ("SAPPD", 26),
    ("INFSED", 27),
    ("PINNED", 28),
    ("CONCUSED", 29),
];

fn effect_index(effect: &str) -> Option<u8> {
    EFFECT_INDEX
        .iter()
        .find(|(name, _)| *name == effect)
        .map(|(_, v)| *v)
}

/// Go `moves.go` `accuracyIndex`: accuracy -> 2-bit index (unknown -> 0).
fn accuracy_index(accuracy: i32) -> u16 {
    match accuracy {
        100 => 0,
        90 => 1,
        80 => 2,
        70 => 3,
        _ => 0,
    }
}

/// Mirrors Go `BuildCreatures`: produces the `creatureData.bin` bytes and
/// the `creatureData.hpp` text for the given creatures.
pub fn build_creatures(creatures: &[Creature]) -> Result<(Vec<u8>, String), BuildError> {
    let mut bin = Vec::with_capacity(creatures.len() * 14);
    let mut table = String::new();
    table.push_str("namespace CreatureData {\n\n    uint8_t creatureData[] = {\n");
    for c in creatures {
        let t1 = creature_type_byte(&c.type1)
            .ok_or_else(|| BuildError(format!("unknown type for creature {}", c.name)))?;
        let t2 = creature_type_byte(&c.type2)
            .ok_or_else(|| BuildError(format!("unknown type for creature {}", c.name)))?;
        bin.extend_from_slice(&[
            c.id as u8,
            t1,
            t2,
            c.evo_level as u8,
            c.atk as u8,
            c.def as u8,
            c.spc_atk as u8,
            c.spc_def as u8,
            c.hp as u8,
            c.spd as u8,
            c.move1 as u8,
            c.move2 as u8,
            c.move3 as u8,
            c.move4 as u8,
        ]);
        table.push_str(&format!(
            "    {{{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}}},\n",
            c.id,
            t1,
            t2,
            c.evo_level,
            c.atk,
            c.def,
            c.spc_atk,
            c.spc_def,
            c.hp,
            c.spd,
            c.move1,
            c.move2,
            c.move3,
            c.move4
        ));
    }
    table.push_str("    };\n\n}\n");
    Ok((bin, table))
}

/// Mirrors Go `BuildMoves`: produces the `moves.bin` bytes and the
/// `moves.hpp` text for the given moves.
pub fn build_moves(moves: &[Move]) -> Result<(Vec<u8>, String), BuildError> {
    let mut bin = Vec::with_capacity(moves.len() * 4);
    let mut hpp = String::from("namespace MoveData {\n");
    for m in moves {
        let t = move_type_index(&m.r#type).ok_or_else(|| {
            BuildError(format!(
                "unknown move type {:?} for move {}",
                m.r#type, m.name
            ))
        })?;
        let acc = accuracy_index(m.accuracy);
        let packed =
            ((t as u16) << 12) | ((m.power as u16) << 7) | ((m.physical as u16) << 6) | (acc << 4);
        let eff = effect_index(&m.effect).unwrap_or(0xFF);
        bin.extend_from_slice(&packed.to_be_bytes());
        bin.push(eff);
        bin.push(0xFF); // effect2 always NONE
        hpp.push_str(&format!(
            "    uint8_t move{}[] = \"{}\\0\";\n",
            m.id, m.name
        ));
    }
    hpp.push_str("\n    uint24_t moveNames[] = {\n        ");
    for (i, m) in moves.iter().enumerate() {
        if i > 0 {
            hpp.push_str(", ");
        }
        hpp.push_str(&format!("move{}", m.id));
    }
    hpp.push_str("\n    };\n\n}\n");
    Ok((bin, hpp))
}

/// Mirrors Go `BuildCreatures` file side: writes `creatureData.bin` and
/// `creatureData.hpp` into `cfg.output_dir` (creating it if needed).
pub fn write_creatures(cfg: &Config, creatures: &[Creature]) -> Result<(), BuildError> {
    let (bin, hpp) = build_creatures(creatures)?;
    let dir = Path::new(&cfg.output_dir);
    std::fs::create_dir_all(dir).map_err(|e| io_err("creating", dir, e))?;
    let bin_path = dir.join("creatureData.bin");
    let hpp_path = dir.join("creatureData.hpp");
    std::fs::write(&bin_path, &bin).map_err(|e| io_err("writing", &bin_path, e))?;
    std::fs::write(&hpp_path, hpp).map_err(|e| io_err("writing", &hpp_path, e))?;
    Ok(())
}

/// Mirrors Go `BuildMoves` file side: writes `moves.bin` and `moves.hpp`
/// into `cfg.output_dir` (creating it if needed).
pub fn write_moves(cfg: &Config, moves: &[Move]) -> Result<(), BuildError> {
    let (bin, hpp) = build_moves(moves)?;
    let dir = Path::new(&cfg.output_dir);
    std::fs::create_dir_all(dir).map_err(|e| io_err("creating", dir, e))?;
    let bin_path = dir.join("moves.bin");
    let hpp_path = dir.join("moves.hpp");
    std::fs::write(&bin_path, &bin).map_err(|e| io_err("writing", &bin_path, e))?;
    std::fs::write(&hpp_path, hpp).map_err(|e| io_err("writing", &hpp_path, e))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Map builder, ported from creatureTools internal/builder/mapbuilder.go.
// Go output is the byte-for-byte reference contract:
//
// - `raw_map.bin` : 256×256 u16 LE tiles, row-major, out-of-range → 0.
// - `scripts.bin` : 2048 chunks × 128 bytes, each non-empty chunk is the
//   concatenated compiled bytecode followed by an End (0xFF) sentinel.
// - `text.bin`    : u16 LE count + u16 LE offsets + u16 LE len-prefixed
//   strings.
// - `.hpp` files : opcodes / flags / flag bit-array / locations / scripts
//   in Go's exact layout (sorted names; scripts.hpp embeds the raw script
//   sources as comments and the compiled blob (no End) as byte arrays).
//
// The script compiler is `crate::script` (ported from Go internal/script);
// `@name` location references resolve against an EMPTY map exactly like Go
// mapbuilder.go, which builds `locations := map[string][2]uint16{}`.
// ─────────────────────────────────────────────────────────────────────

/// Chunk payload size in `scripts.bin` (Go `chunkSize = 128`).
const CHUNK_SIZE: usize = 128;

/// Every artifact a map build produces (mirrors the files Go `BuildMap`
/// writes into the output directory).
#[derive(Debug)]
pub struct MapArtifacts {
    pub raw_map: Vec<u8>,
    pub scripts_bin: Vec<u8>,
    pub text_bin: Vec<u8>,
    pub opcodes_hpp: String,
    pub flags_hpp: String,
    pub flag_bit_array_hpp: String,
    pub flag_bit_array_cpp: String,
    pub locations_hpp: String,
    pub scripts_hpp: String,
}

fn opcodes_hpp_text() -> String {
    String::from(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
         enum class VmOpcode : uint8_t {\n\
         \x20   Msg = 0,\n\
         \x20   TMsg = 1,\n\
         \x20   SMsg = 2,\n\
         \x20   Tp = 3,\n\
         \x20   TpIf = 4,\n\
         \x20   If = 5,\n\
         \x20   SetFlag = 6,\n\
         \x20   UnsetFlag = 7,\n\
         \x20   ReadFlag = 8,\n\
         \x20   End = 255,\n\
         };\n",
    )
}

/// Go `buildTextBin`: u16 LE count, u16 LE per-text offset, then each text
/// as u16 LE byte-length + bytes.
fn build_text_bin(texts: &[String]) -> Vec<u8> {
    let mut offsets = Vec::with_capacity(texts.len());
    let mut blob = Vec::new();
    let mut cursor: u16 = 0;
    for t in texts {
        offsets.push(cursor);
        let ascii = t.as_bytes();
        blob.extend_from_slice(&(ascii.len() as u16).to_le_bytes());
        blob.extend_from_slice(ascii);
        cursor += 2 + ascii.len() as u16;
    }

    let mut out = Vec::new();
    out.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
    for off in offsets {
        out.extend_from_slice(&off.to_le_bytes());
    }
    out.extend_from_slice(&blob);
    out
}

/// Go `writeFlagsHPP` / `writeLocationsHPP`: sorted names, one
/// `uint16_t name = id;` line each, header first.
fn name_id_hpp(header: &str, table: &HashMap<String, u16>, prefix: &str) -> String {
    let mut h = String::from(header);
    let mut names: Vec<&String> = table.keys().collect();
    names.sort();
    for name in names {
        h.push_str(&format!("{prefix}{name} = {};\n", table[name]));
    }
    h
}

/// Go `writeScriptsHPP`: per non-empty chunk, optional `// source` comment
/// (raw script text) then a `uint8_t blobN[] = { b0,b1,... };` line.
/// Note: the compiled blob does NOT include the trailing 0xFF End that is
/// appended only inside `scripts.bin`.
fn scripts_hpp_text(blobs: &[Vec<u8>], scripts: &[String]) -> String {
    let mut h =
        String::from("#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n");
    for (i, blob) in blobs.iter().enumerate() {
        if blob.is_empty() {
            continue;
        }
        if !scripts[i].is_empty() {
            h.push_str(&format!("// {}\n", scripts[i]));
        }
        let nums: Vec<String> = blob.iter().map(|b| b.to_string()).collect();
        h.push_str(&format!(
            "uint8_t blob{}[] = {{ {} }};\n",
            i,
            nums.join(",")
        ));
    }
    h
}

/// Mirrors Go `BuildMap`: compiles the map's NPC + script-object scripts
/// into per-chunk bytecode and produces every output artifact in memory.
pub fn build_map(map: &GameMap) -> Result<MapArtifacts, BuildError> {
    use crate::model::{CHUNK_COLS, CHUNK_H, CHUNK_W, MAP_H, MAP_W, TOTAL_CHUNKS};

    // raw_map.bin: 256×256 u16 tiles, row-major, little-endian.
    let mut raw_map = vec![0u8; (MAP_W * MAP_H * 2) as usize];
    for row in 0..MAP_H {
        for col in 0..MAP_W {
            let tile = map
                .tiles
                .get(row as usize)
                .and_then(|trow| trow.get(col as usize))
                .copied()
                .unwrap_or(0);
            let off = ((row * MAP_W + col) * 2) as usize;
            raw_map[off] = (tile & 0xFF) as u8;
            raw_map[off + 1] = (tile >> 8) as u8;
        }
    }

    // Collect every non-empty script by chunk index (Go entry order:
    // NPCs first, then script objects).
    struct ScriptEntry {
        chunk_idx: usize,
        src: String,
    }
    let mut entries = Vec::new();
    for npc in &map.npcs {
        if !npc.script.is_empty() {
            let idx = ((npc.y / CHUNK_H) * CHUNK_COLS + npc.x / CHUNK_W) as usize;
            entries.push(ScriptEntry {
                chunk_idx: idx,
                src: npc.script.clone(),
            });
        }
    }
    for sc in &map.scripts {
        if !sc.script.is_empty() {
            let idx = ((sc.y / CHUNK_H) * CHUNK_COLS + sc.x / CHUNK_W) as usize;
            entries.push(ScriptEntry {
                chunk_idx: idx,
                src: sc.script.clone(),
            });
        }
    }

    // Compile context. Go builds `locations := map[string][2]uint16{}`
    // (empty) — mirror exactly.
    let mut ctrl = script::Controller::new();
    let locations: HashMap<String, (u16, u16)> = HashMap::new();

    let mut chunk_blobs: Vec<Vec<u8>> = vec![Vec::new(); TOTAL_CHUNKS];
    let mut chunk_scripts: Vec<String> = vec![String::new(); TOTAL_CHUNKS];
    for e in &entries {
        let cmds = script::parse(&e.src, &locations, &mut ctrl)
            .map_err(|err| BuildError(format!("parse error in chunk {}: {}", e.chunk_idx, err)))?;
        let mut blob = Vec::new();
        for cmd in &cmds {
            blob.extend_from_slice(&cmd.to_bytes());
        }
        chunk_blobs[e.chunk_idx].extend_from_slice(&blob);
        chunk_scripts[e.chunk_idx].push_str(&e.src);
    }

    // scripts.bin: 2048 chunks × 128 bytes (padded), each non-empty chunk
    // ends with an End (0xFF) sentinel.
    let mut scripts_bin = vec![0u8; TOTAL_CHUNKS * CHUNK_SIZE];
    for i in 0..TOTAL_CHUNKS {
        let mut blob = chunk_blobs[i].clone();
        if blob.is_empty() {
            continue;
        }
        blob.push(0xFF);
        if blob.len() > CHUNK_SIZE {
            return Err(BuildError(format!(
                "chunk {} too large: {} bytes > {}",
                i,
                blob.len(),
                CHUNK_SIZE
            )));
        }
        scripts_bin[i * CHUNK_SIZE..i * CHUNK_SIZE + blob.len()].copy_from_slice(&blob);
    }

    // text.bin uses the controller's deduplicated texts indexed by id.
    let mut sorted_texts: Vec<String> = vec![String::new(); ctrl.texts.len()];
    for (text, &idx) in &ctrl.texts {
        sorted_texts[idx as usize] = text.clone();
    }
    let text_bin = build_text_bin(&sorted_texts);

    let opcodes_hpp = opcodes_hpp_text();
    let flags_hpp = name_id_hpp(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n",
        &ctrl.flags,
        "uint16_t ",
    );
    let bits = ctrl.flags.len();
    let bytes = bits / 8 + usize::from(bits % 8 != 0);
    let flag_bit_array_hpp = format!(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nextern uint8_t FLAG_BIT_ARRAY[{bytes}];\n"
    );
    let flag_bit_array_cpp = format!(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nuint8_t FLAG_BIT_ARRAY[{bytes}] = {{0}};\n"
    );
    let locations_hpp = name_id_hpp(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n",
        &ctrl.tags,
        "uint16_t ",
    );
    let scripts_hpp = scripts_hpp_text(&chunk_blobs, &chunk_scripts);

    Ok(MapArtifacts {
        raw_map,
        scripts_bin,
        text_bin,
        opcodes_hpp,
        flags_hpp,
        flag_bit_array_hpp,
        flag_bit_array_cpp,
        locations_hpp,
        scripts_hpp,
    })
}

/// Mirrors Go `BuildMap` file side: writes all map artifacts into
/// `cfg.output_dir` (creating it if needed).
pub fn write_map(cfg: &Config, map: &GameMap) -> Result<(), BuildError> {
    let a = build_map(map)?;
    let dir = Path::new(&cfg.output_dir);
    std::fs::create_dir_all(dir).map_err(|e| io_err("creating", dir, e))?;
    write_bytes(&dir.join("raw_map.bin"), &a.raw_map)?;
    write_bytes(&dir.join("scripts.bin"), &a.scripts_bin)?;
    write_bytes(&dir.join("text.bin"), &a.text_bin)?;
    write_str(&dir.join("opcodes.hpp"), &a.opcodes_hpp)?;
    write_str(&dir.join("flags.hpp"), &a.flags_hpp)?;
    write_str(&dir.join("flag_bit_array.hpp"), &a.flag_bit_array_hpp)?;
    write_str(&dir.join("flag_bit_array.cpp"), &a.flag_bit_array_cpp)?;
    write_str(&dir.join("locations.hpp"), &a.locations_hpp)?;
    write_str(&dir.join("scripts.hpp"), &a.scripts_hpp)?;
    Ok(())
}

fn write_bytes(path: &Path, data: &[u8]) -> Result<(), BuildError> {
    std::fs::write(path, data).map_err(|e| io_err("writing", path, e))
}

fn write_str(path: &Path, data: &str) -> Result<(), BuildError> {
    std::fs::write(path, data).map_err(|e| io_err("writing", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NPCObject, ScriptObject};
    use crate::store::Store;

    const TD: &str = env!("CARGO_MANIFEST_DIR");
    const CREATURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/creatures.json");
    const MOVES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/moves.json");

    fn fixture_cfg(output_dir: &str) -> Config {
        Config {
            creatures_json: CREATURES.to_string(),
            moves_json: MOVES.to_string(),
            map_json: concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/map.json").to_string(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: format!("{TD}/testdata/builder_out/{output_dir}"),
            fixture_dir: None,
        }
    }

    fn fixture_creatures() -> Vec<Creature> {
        let cfg = fixture_cfg("_");
        Store::new(&cfg).load_creatures().expect("fixture parses")
    }

    fn fixture_moves() -> Vec<Move> {
        let cfg = fixture_cfg("_");
        Store::new(&cfg).load_moves().expect("fixture parses")
    }

    // ─── creatures.bin: exact bytes from the Go builder (reference contract) ───
    // SquibbleSnail: wind=2, none=8 -> {0,2,8,8,3,3,2,2,3,3,8,32,32,32}
    // NewOne:        none=8, none=8 -> {99,8,8,1,1,1,1,1,1,1,0,0,0,0}
    const EXPECTED_CREATURES_BIN: [u8; 28] = [
        0, 2, 8, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32, //
        99, 8, 8, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0,
    ];

    const EXPECTED_CREATURES_HPP: &str = r#"namespace CreatureData {

    uint8_t creatureData[] = {
    {0, 2, 8, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32},
    {99, 8, 8, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0},
    };

}
"#;

    #[test]
    fn build_creatures_bin_size() {
        let creatures = fixture_creatures();
        let (bin, _) = build_creatures(&creatures).expect("build");
        assert_eq!(bin.len(), creatures.len() * 14);
    }

    #[test]
    fn build_creatures_bin_matches_go_reference() {
        let creatures = fixture_creatures();
        let (bin, _) = build_creatures(&creatures).expect("build");
        assert_eq!(bin, EXPECTED_CREATURES_BIN.to_vec());
    }

    #[test]
    fn build_creatures_first_entry() {
        let creatures = fixture_creatures();
        let (bin, _) = build_creatures(&creatures).expect("build");
        // SquibbleSnail (Go TestBuildCreaturesFirstEntry).
        let expected: [u8; 14] = [0, 2, 8, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32];
        assert_eq!(&bin[..14], &expected);
    }

    #[test]
    fn build_creatures_hpp_contains() {
        let creatures = fixture_creatures();
        let (_, hpp) = build_creatures(&creatures).expect("build");
        assert!(hpp.contains("creatureData[]"));
    }

    #[test]
    fn build_creatures_hpp_matches_go_reference() {
        let creatures = fixture_creatures();
        let (_, hpp) = build_creatures(&creatures).expect("build");
        assert_eq!(hpp, EXPECTED_CREATURES_HPP);
    }

    #[test]
    fn build_creatures_unknown_type_errors() {
        let mut creatures = fixture_creatures();
        creatures[0].type1 = "bogus".to_string();
        let err = build_creatures(&creatures).expect_err("must error");
        assert!(err.0.contains("unknown type for creature SquibbleSnail"));
    }

    // ─── moves.bin: exact bytes from the Go builder (reference contract) ───
    // smite:   type spirit=0, power=10, physical=0, acc=100 -> packed=1280=0x0500
    //          -> [0x05, 0x00, 0xFF(eff NONE), 0xFF(effect2)]
    // thought: type spirit=0, power=5,  physical=1, acc=100 -> packed=704=0x02C0
    //          -> [0x02, 0xC0, 0xFF, 0xFF]
    const EXPECTED_MOVES_BIN: [u8; 8] = [0x05, 0x00, 0xFF, 0xFF, 0x02, 0xC0, 0xFF, 0xFF];

    const EXPECTED_MOVES_HPP: &str = r#"namespace MoveData {
    uint8_t move0[] = "smite\0";
    uint8_t move1[] = "thought\0";

    uint24_t moveNames[] = {
        move0, move1
    };

}
"#;

    #[test]
    fn build_moves_bin_size() {
        let moves = fixture_moves();
        let (bin, _) = build_moves(&moves).expect("build");
        assert_eq!(bin.len(), moves.len() * 4);
    }

    #[test]
    fn build_moves_bin_matches_go_reference() {
        let moves = fixture_moves();
        let (bin, _) = build_moves(&moves).expect("build");
        assert_eq!(bin, EXPECTED_MOVES_BIN.to_vec());
    }

    #[test]
    fn build_moves_hpp_contains() {
        let moves = fixture_moves();
        let (_, hpp) = build_moves(&moves).expect("build");
        assert!(hpp.contains("move0[]"));
        assert!(hpp.contains("moveNames[]"));
    }

    #[test]
    fn build_moves_hpp_matches_go_reference() {
        let moves = fixture_moves();
        let (_, hpp) = build_moves(&moves).expect("build");
        assert_eq!(hpp, EXPECTED_MOVES_HPP);
    }

    #[test]
    fn build_moves_unknown_type_errors() {
        let mut moves = fixture_moves();
        moves[0].r#type = "void".to_string();
        let err = build_moves(&moves).expect_err("must error");
        assert!(err.0.contains("unknown move type \"void\" for move smite"));
    }

    #[test]
    fn build_moves_none_type_is_15() {
        let moves = vec![Move {
            id: 7,
            name: "stall".to_string(),
            r#type: "none".to_string(),
            power: 0,
            physical: 0,
            accuracy: 100,
            effect: "NONE".to_string(),
        }];
        let (bin, _) = build_moves(&moves).expect("build");
        // packed = 15<<12 | 0<<7 | 0<<6 | 0<<4 = 0xF000
        assert_eq!(bin, vec![0xF0, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn build_moves_known_effect_soaked() {
        let moves = vec![Move {
            id: 2,
            name: "splash".to_string(),
            r#type: "water".to_string(),
            power: 3,
            physical: 0,
            accuracy: 90,
            effect: "SOAKED".to_string(),
        }];
        let (bin, _) = build_moves(&moves).expect("build");
        // t=1 -> 1<<12=0x1000; power 3<<7=0x180; acc 90->1 <<4=0x10 -> 0x1190
        assert_eq!(bin[..2], [0x11, 0x90]);
        assert_eq!(bin[2], 1); // SOAKED
        assert_eq!(bin[3], 0xFF); // effect2
    }

    #[test]
    fn build_moves_unknown_effect_defaults_ff() {
        let mut moves = fixture_moves();
        moves[0].effect = "MYSTERY".to_string();
        let (bin, _) = build_moves(&moves).expect("build");
        assert_eq!(bin[2], 0xFF);
    }

    #[test]
    fn build_moves_unknown_accuracy_defaults_zero() {
        let mut moves = fixture_moves();
        moves[0].accuracy = 55; // not in Go accuracyIndex
        let (bin, _) = build_moves(&moves).expect("build");
        // smite power=10, physical=0: packed = 0<<12|10<<7|0<<6|0<<4 = 0x0500
        assert_eq!(bin[..2], [0x05, 0x00]);
    }

    // ─── Go-style file writing into cfg.output_dir ───
    #[test]
    fn write_creatures_outputs_files() {
        let cfg = fixture_cfg("creatures");
        let creatures = fixture_creatures();
        write_creatures(&cfg, &creatures).expect("write");
        let bin = std::fs::read(format!("{}/creatureData.bin", cfg.output_dir)).expect("read bin");
        let hpp = std::fs::read_to_string(format!("{}/creatureData.hpp", cfg.output_dir))
            .expect("read hpp");
        assert_eq!(bin, EXPECTED_CREATURES_BIN.to_vec());
        assert_eq!(hpp, EXPECTED_CREATURES_HPP);
    }

    #[test]
    fn write_moves_outputs_files() {
        let cfg = fixture_cfg("moves");
        let moves = fixture_moves();
        write_moves(&cfg, &moves).expect("write");
        let bin = std::fs::read(format!("{}/moves.bin", cfg.output_dir)).expect("read bin");
        let hpp =
            std::fs::read_to_string(format!("{}/moves.hpp", cfg.output_dir)).expect("read hpp");
        assert_eq!(bin, EXPECTED_MOVES_BIN.to_vec());
        assert_eq!(hpp, EXPECTED_MOVES_HPP);
    }

    // ─── map builder: ported mapbuilder_test.go (reference contract) ───

    fn empty_map() -> GameMap {
        GameMap {
            width: 256,
            height: 256,
            tiles: vec![vec![0u16; 256]; 256],
            npcs: vec![],
            scripts: vec![],
            collision: vec![],
            collision_types: vec![],
            encounters: vec![],
            encounter_zones: vec![],
        }
    }

    #[test]
    fn build_map_empty_raw_bin_all_zero() {
        let a = build_map(&empty_map()).expect("build");
        assert_eq!(a.raw_map.len(), 131072, "expected 131072 bytes");
        assert!(a.raw_map.iter().all(|&b| b == 0), "expected all zeros");
    }

    #[test]
    fn build_map_raw_bin_tiles_at_positions() {
        let mut m = empty_map();
        m.tiles[0][0] = 0x0102;
        m.tiles[1][2] = 0x1234;
        m.tiles[7][255] = 0xABCD;
        let a = build_map(&m).expect("build");
        // LE row-major: offset (row*256+col)*2.
        let off = |row: usize, col: usize| (row * 256 + col) * 2;
        assert_eq!(&a.raw_map[off(0, 0)..off(0, 0) + 2], &[0x02, 0x01]);
        assert_eq!(&a.raw_map[off(1, 2)..off(1, 2) + 2], &[0x34, 0x12]);
        assert_eq!(&a.raw_map[off(7, 255)..off(7, 255) + 2], &[0xCD, 0xAB]);
    }

    #[test]
    fn build_map_out_of_bounds_tiles_are_zero() {
        // Go: raw map iterates the fixed 256×256 grid; rows/cols beyond the
        // tile matrix read as 0.
        let mut m = empty_map();
        m.tiles = vec![vec![9u16; 3]; 2]; // 2×3 map inside a 256×256 grid
        let a = build_map(&m).expect("build");
        assert_eq!(a.raw_map.len(), 131072);
        assert_eq!(&a.raw_map[..2], &[9, 0]);
        assert_eq!(&a.raw_map[2..4], &[9, 0]);
        assert_eq!(&a.raw_map[2 * 256 * 2..2 * 256 * 2 + 2], &[0, 0]);
        assert_eq!(&a.raw_map[255 * 256 * 2..255 * 256 * 2 + 2], &[0, 0]);
    }

    #[test]
    fn build_map_chunk_index_00_msg_opcode() {
        // Go TestChunkIndex00: script at (0,0) lands in chunk 0; "msg {Hi}"
        // compiles to [0, 0, 0] + 0xFF End.
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        let chunk_base = 0 * CHUNK_SIZE;
        assert_eq!(
            a.scripts_bin[chunk_base], 0,
            "expected Msg opcode 0 at chunk 0"
        );
        assert_eq!(&a.scripts_bin[chunk_base..chunk_base + 4], &[0, 0, 0, 0xFF]);
    }

    #[test]
    fn build_map_chunk_index_33_setflag_opcode() {
        // Go TestChunkIndex33: (8,4) → (8/8)*32 + (4/4) = 33.
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 8,
            y: 4,
            script: "setflag flag_myflag".to_string(),
        }];
        let a = build_map(&m).expect("build");
        let chunk_base = 33 * CHUNK_SIZE;
        assert_eq!(
            a.scripts_bin[chunk_base], 6,
            "expected SetFlag opcode 6 at chunk 33"
        );
        assert_eq!(&a.scripts_bin[chunk_base..chunk_base + 4], &[6, 0, 0, 0xFF]);
    }

    #[test]
    fn build_map_scripts_bin_multiple_of_128() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hello world}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        assert_eq!(a.scripts_bin.len() % 128, 0, "not multiple of 128");
        assert_eq!(a.scripts_bin.len(), 2048 * 128, "expected 262144 bytes");
    }

    #[test]
    fn build_map_npc_script_collected_before_script_objects() {
        // NPCs are appended to chunks before script objects (Go order),
        // so both compile into the same chunk blob in that order.
        let mut m = empty_map();
        m.npcs = vec![NPCObject {
            id: 1,
            x: 0,
            y: 0,
            sprite_id: 1,
            script: "msg {Hi}".to_string(),
        }];
        m.scripts = vec![ScriptObject {
            id: 2,
            x: 0,
            y: 0,
            script: "setflag flag_door".to_string(),
        }];
        let a = build_map(&m).expect("build");
        // msg bytes + setflag bytes + End: [0,0,0] [6,0,0] [0xFF]
        assert_eq!(&a.scripts_bin[..7], &[0, 0, 0, 6, 0, 0, 0xFF]);
    }

    #[test]
    fn build_map_empty_scripts_skipped() {
        let mut m = empty_map();
        m.scripts = vec![
            ScriptObject {
                id: 1,
                x: 0,
                y: 0,
                script: String::new(),
            },
            ScriptObject {
                id: 2,
                x: 0,
                y: 0,
                script: "msg {Hi}".to_string(),
            },
        ];
        m.npcs = vec![NPCObject {
            id: 3,
            x: 0,
            y: 0,
            sprite_id: 1,
            script: "".to_string(),
        }];
        let a = build_map(&m).expect("build");
        assert_eq!(&a.scripts_bin[..4], &[0, 0, 0, 0xFF]);
        assert_eq!(a.scripts_bin[4], 0, "rest of chunk stays zero-padded");
    }

    #[test]
    fn build_map_text_bin_matches_go_layout() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        // count=1 LE, offset[0]=0 LE, len=2 LE, "Hi"
        assert_eq!(a.text_bin, vec![1, 0, 0, 0, 2, 0, 0x48, 0x69]);
    }

    #[test]
    fn build_map_text_bin_offsets_accumulate() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {ab}; msg {cd}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        // count=2; text0 offset 0; text1 offset 2+2=4; then len/bytes pairs
        // (lengths are u16 LE: len=2 -> [2, 0]).
        assert_eq!(
            a.text_bin,
            vec![2, 0, 0, 0, 4, 0, 2, 0, b'a', b'b', 2, 0, b'c', b'd']
        );
    }

    #[test]
    fn build_map_hpp_outputs_match_go() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        m.npcs = vec![NPCObject {
            id: 2,
            x: 8,
            y: 4,
            sprite_id: 1,
            script: "setflag flag_myflag".to_string(),
        }];
        let a = build_map(&m).expect("build");

        assert_eq!(
            a.opcodes_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
             enum class VmOpcode : uint8_t {\n\
             \x20   Msg = 0,\n\
             \x20   TMsg = 1,\n\
             \x20   SMsg = 2,\n\
             \x20   Tp = 3,\n\
             \x20   TpIf = 4,\n\
             \x20   If = 5,\n\
             \x20   SetFlag = 6,\n\
             \x20   UnsetFlag = 7,\n\
             \x20   ReadFlag = 8,\n\
             \x20   End = 255,\n\
             };\n"
        );
        // flags.hpp: single flag, sorted, `uint16_t name = id;`.
        assert_eq!(
            a.flags_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nuint16_t flag_myflag = 0;\n"
        );
        // locations.hpp: no tag used → header only.
        assert_eq!(
            a.locations_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n"
        );
        // flag bit array: 1 flag → 1 byte.
        assert_eq!(
            a.flag_bit_array_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nextern uint8_t FLAG_BIT_ARRAY[1];\n"
        );
        assert_eq!(
            a.flag_bit_array_cpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nuint8_t FLAG_BIT_ARRAY[1] = {0};\n"
        );
        // scripts.hpp: chunk 0 (msg blob, no End) with source comment.
        assert_eq!(
            a.scripts_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
             // msg {Hi}\n\
             uint8_t blob0[] = { 0,0,0 };\n\
             // setflag flag_myflag\n\
             uint8_t blob33[] = { 6,0,0 };\n"
        );
    }

    #[test]
    fn build_map_parse_error_reports_chunk() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "nope".to_string(),
        }];
        let err = build_map(&m).expect_err("must fail");
        assert!(err.0.contains("parse error in chunk 0"), "err: {}", err.0);
    }

    #[test]
    fn build_map_chunk_too_large_errors() {
        let mut m = empty_map();
        // Each `msg {xxxxxxxxxx}` compiles to 3 bytes; 45 of them (135 bytes)
        // plus the 0xFF End sentinel overflow the 128-byte chunk capacity
        // (Go behavior).
        let script = std::iter::repeat_n("msg {xxxxxxxxxx}".to_string(), 45)
            .collect::<Vec<_>>()
            .join("; ");
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script,
        }];
        let err = build_map(&m).expect_err("must fail");
        assert!(err.0.contains("chunk 0 too large"), "err: {}", err.0);
    }

    #[test]
    fn write_map_outputs_files() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        let cfg = fixture_cfg("map");
        let a = build_map(&m).expect("build");
        write_map(&cfg, &m).expect("write");
        for (name, bytes) in [
            ("raw_map.bin", a.raw_map),
            ("scripts.bin", a.scripts_bin),
            ("text.bin", a.text_bin),
        ] {
            let disk = std::fs::read(format!("{}/{}", cfg.output_dir, name)).expect("read file");
            assert_eq!(disk, bytes, "file {name} mismatch");
        }
        for (name, text) in [
            ("opcodes.hpp", a.opcodes_hpp),
            ("flags.hpp", a.flags_hpp),
            ("flag_bit_array.hpp", a.flag_bit_array_hpp),
            ("flag_bit_array.cpp", a.flag_bit_array_cpp),
            ("locations.hpp", a.locations_hpp),
            ("scripts.hpp", a.scripts_hpp),
        ] {
            let disk =
                std::fs::read_to_string(format!("{}/{}", cfg.output_dir, name)).expect("read hpp");
            assert_eq!(disk, text, "file {name} mismatch");
        }
    }

    #[test]
    fn build_map_parity_fixture_matches_go_reference() {
        // Byte-identical-to-Go fixture: hand-computed from Go mapbuilder.go.
        let mut m = empty_map();
        m.tiles[0][0] = 5;
        m.tiles[0][1] = 0x0102;
        m.tiles[3][10] = 0xFFFF;
        m.npcs = vec![NPCObject {
            id: 10,
            x: 0,
            y: 0,
            sprite_id: 3,
            script: "msg {Hi}".to_string(),
        }];
        m.scripts = vec![
            ScriptObject {
                id: 20,
                x: 8,
                y: 4,
                script: "setflag flag_myflag".to_string(),
            },
            ScriptObject {
                id: 21,
                x: 240,
                y: 12,
                script: "msg {Bye}".to_string(),
            },
        ];
        let a = build_map(&m).expect("build");

        // raw_map.bin
        assert_eq!(a.raw_map.len(), 131072);
        assert_eq!(&a.raw_map[0..2], &[5, 0]);
        assert_eq!(&a.raw_map[2..4], &[0x02, 0x01]);
        assert_eq!(
            &a.raw_map[(3 * 256 + 10) * 2..(3 * 256 + 10) * 2 + 2],
            &[0xFF, 0xFF]
        );
        assert_eq!(a.raw_map[4], 0);
        assert_eq!(&a.raw_map[131070..], &[0, 0]);

        // chunk 0 (0,0): msg {Hi} → [0,0,0] + 0xFF
        assert_eq!(&a.scripts_bin[0..4], &[0, 0, 0, 0xFF]);
        // chunk 33 (8,4): setflag flag_myflag → [6,0,0] + 0xFF
        let c33 = 33 * CHUNK_SIZE;
        assert_eq!(&a.scripts_bin[c33..c33 + 4], &[6, 0, 0, 0xFF]);
        // chunk (12/4=3)*32 + (240/8=30) = 126: msg {Bye} → [0,0,1] + 0xFF
        // (text "Bye" is the second deduplicated text, id 1)
        let c126 = 126 * CHUNK_SIZE;
        assert_eq!(&a.scripts_bin[c126..c126 + 4], &[0, 0, 1, 0xFF]);
        // untouched chunk stays zero
        let c100 = 100 * CHUNK_SIZE;
        assert_eq!(&a.scripts_bin[c100..c100 + 4], &[0, 0, 0, 0]);

        // text.bin: "Hi"=0, "Bye"=1 → count 2, offsets 0 and 4, then
        // LE length + bytes pairs.
        assert_eq!(
            a.text_bin,
            vec![2, 0, 0, 0, 4, 0, 2, 0, b'H', b'i', 3, 0, b'B', b'y', b'e']
        );

        // scripts.hpp mirrors chunk blobs (no End), sources as comments.
        assert_eq!(
            a.scripts_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
             // msg {Hi}\n\
             uint8_t blob0[] = { 0,0,0 };\n\
             // setflag flag_myflag\n\
             uint8_t blob33[] = { 6,0,0 };\n\
             // msg {Bye}\n\
             uint8_t blob126[] = { 0,0,1 };\n"
        );
    }
}
