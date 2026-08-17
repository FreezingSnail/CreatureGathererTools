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
use crate::model::{Creature, Move};
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
            c.id, t1, t2, c.evo_level, c.atk, c.def, c.spc_atk, c.spc_def, c.hp, c.spd,
            c.move1, c.move2, c.move3, c.move4
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
        let t = move_type_index(&m.r#type)
            .ok_or_else(|| BuildError(format!("unknown move type {:?} for move {}", m.r#type, m.name)))?;
        let acc = accuracy_index(m.accuracy);
        let packed = ((t as u16) << 12)
            | ((m.power as u16) << 7)
            | ((m.physical as u16) << 6)
            | (acc << 4);
        let eff = effect_index(&m.effect).unwrap_or(0xFF);
        bin.extend_from_slice(&packed.to_be_bytes());
        bin.push(eff);
        bin.push(0xFF); // effect2 always NONE
        hpp.push_str(&format!("    uint8_t move{}[] = \"{}\\0\";\n", m.id, m.name));
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

#[cfg(test)]
mod tests {
    use super::*;
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
        let bin = std::fs::read(format!("{}/creatureData.bin", cfg.output_dir))
            .expect("read bin");
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
}