//! End-to-end parity vs the legacy Go pipeline (`CreatureGathererFX-179.11`).
//!
//! Reference contract: creaturevm-rust bytecode/writer + `crate::builder`
//! must produce byte-identical artifacts to the Go builders
//! (`creatureTools/internal/builder/*`, `internal/script` `Command2Bytes`)
//! for the same input. Expected bytes below are hand-computed from the Go
//! sources (mapbuilder.go, creatures.go, moves.go, script_test.go) — not
//! from the Rust implementation, so the comparison is against the legacy
//! contract, not a self-check.
//!
//! Covered here:
//! 1. Full map pipeline: `GameMap` fixture → `builder::build_map` → every
//!    artifact (`raw_map.bin`, `scripts.bin`, `text.bin`, all `.hpp`s),
//!    compared byte-for-byte with the Go-built bytes, including a map that
//!    exercises every DSL command (msg/tmsg/smsg/tp/tpif/if/sf/usf/rf).
//! 2. Empty map parity (no NPCs, no scripts).
//! 3. Creatures pipeline: `Store` fixture load → `build_creatures` → exact Go
//!    reference bytes.
//! 4. Tiled interchange end-to-end on the original creaturevm-rust fixture
//!    (`tests/world_map.json`): `tiled::import` → `tiled::export` →
//!    `tiled::import` round-trips to an equal `GameMap`.
//!
//! Expected discrepancy count: ZERO (any deviation below is a bug).

use std::collections::HashMap;
use std::fs;

use creaturevm::builder::{build_creatures, build_map};
use creaturevm::config::Config;
use creaturevm::model::{GameMap, NPCObject, ScriptObject};
use creaturevm::script::{self, Controller, ToBytecode};
use creaturevm::store::Store;
use creaturevm::tiled;

const TD: &str = env!("CARGO_MANIFEST_DIR");

fn read_fixture(rel: &str) -> String {
    fs::read_to_string(format!("{TD}/{rel}")).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

fn load_creatures() -> Vec<creaturevm::model::Creature> {
    let cfg = Config {
        creatures_json: format!("{TD}/testdata/creatures.json"),
        map_json: format!("{TD}/testdata/map.json"),
        tileset_png: String::new(),
        map_tileset_png: String::new(),
        npc_tileset_png: String::new(),
        output_dir: String::new(),
        fixture_dir: None,
        battle_presets: None,
        arena_demo: None,
        duel: None,
        fx_layout: None,
        blocks_json: None,
        beta_area: None,
    };
    Store::new(&cfg)
        .load_creatures()
        .expect("creatures fixture parses")
}

fn empty_map() -> GameMap {
    GameMap {
        width: 256,
        height: 256,
        tiles: vec![vec![0u16; 256]; 256],
        tilesets: vec![],
        tile_properties: Default::default(),
        source_path: None,
        npcs: vec![],
        scripts: vec![],
        collision: vec![],
        collision_types: vec![],
        encounters: vec![],
        encounter_zones: vec![],
        wfc: None,
        stamps: vec![],
    }
}

fn chunk_base(idx: usize) -> usize {
    idx * 128
}

// ─────────────────────────────────────────────────────────────────────
// 1. Full map pipeline, byte-for-byte Go parity (mapbuilder.go).
// ─────────────────────────────────────────────────────────────────────

/// Every DSL command in one build. Hand-computed from Go
/// `mapbuilder.go` + script `Command2Bytes`:
///
/// Entry order (Go: NPCs first, then script objects), chunk index
/// `(y/4)*32 + (x/8)`:
///   * NPC    @ (0,0)   → chunk 0   "msg {Hello}; setflag flag_f1"
///   * script @ (8,4)   → chunk 33  "tmsg 10 20 {Hello}; unsetflag flag_f1; tp 5 6"
///   * script @ (2,6)   → chunk 32  "if flag_A then msg {IfA} else msg {IfB}
///                                   endif; tpif 7 8 9 10; readflag flag_f2;
///                                   smsg 3 4 {Secret}"
///
/// Controller ids (assignment order): texts Hello=0, IfA=1, IfB=2,
/// Secret=3; flags flag_f1=0, flag_A=1, flag_f2=2.
fn kitchen_sink_map() -> GameMap {
    let mut m = empty_map();
    m.tiles[0][0] = 0x0102;
    m.tiles[3][10] = 0x03FF;
    m.npcs = vec![NPCObject {
        id: 1,
        x: 0,
        y: 0,
        sprite_id: 3,
        script: "msg {Hello}; setflag flag_f1".to_string(),
    }];
    m.scripts = vec![
        ScriptObject {
            id: 2,
            x: 8,
            y: 4,
            script: "tmsg 10 20 {Hello}; unsetflag flag_f1; tp 5 6".to_string(),
        },
        ScriptObject {
            id: 3,
            x: 2,
            y: 6,
            script: "if flag_A then msg {IfA} else msg {IfB} endif; \
                     tpif 7 8 9 10; readflag flag_f2; smsg 3 4 {Secret}"
                .to_string(),
        },
    ];
    m
}

#[test]
fn map_pipeline_all_commands_matches_go_bytes() {
    let m = kitchen_sink_map();
    let a = build_map(&m).expect("build_map");

    // ── raw_map.bin ─────────────────────────────────────────────────
    let mut expected_raw = vec![0u8; 256 * 256 * 2];
    expected_raw[0] = 0x02; // tiles[0][0] = 0x0102, LE
    expected_raw[1] = 0x01;
    let off = (3 * 256 + 10) * 2; // tiles[3][10] = 0x03FF, LE
    expected_raw[off] = 0xFF;
    expected_raw[off + 1] = 0x03;
    assert_eq!(a.raw_map, expected_raw);

    // ── scripts.bin ─────────────────────────────────────────────────
    let mut expected_scripts = vec![0u8; 2048 * 128];
    // chunk 0: [Msg op 0][text "Hello"=0 BE]=[0,0,0] [SetFlag op 6][flag 0 BE]=[6,0,0] + End
    expected_scripts[chunk_base(0)..chunk_base(0) + 7].copy_from_slice(&[0, 0, 0, 6, 0, 0, 0xFF]);
    // chunk 32: If + TpIf + ReadFlag + SMsg
    let chunk32: &[u8] = &[
        5, 0, 0, 1, 0, 3, 0, 0, 1, 0, 0,
        2, // if flag_A then msg{IfA}(idx1) else msg{IfB}(idx2)
        4, 0, 7, 0, 8, 0, 9, 0, 10, // tpif 7 8 9 10
        8, 0, 2, // readflag flag_f2 (idx 2)
        2, 0, 3, 0, 4, 0, 3,    // smsg 3 4 {Secret} (idx 3)
        0xFF, // End
    ];
    expected_scripts[chunk_base(32)..chunk_base(32) + chunk32.len()].copy_from_slice(chunk32);
    // chunk 33: TMsg + UnsetFlag + Tp
    let chunk33: &[u8] = &[
        1, 0, 10, 0, 20, 0, 0, // tmsg 10 20 {Hello}(idx 0)
        7, 0, 0, // unsetflag flag_f1 (idx 0)
        3, 0, 5, 0, 6,    // tp 5 6
        0xFF, // End
    ];
    expected_scripts[chunk_base(33)..chunk_base(33) + chunk33.len()].copy_from_slice(chunk33);
    assert_eq!(a.scripts_bin, expected_scripts);

    // ── text.bin ────────────────────────────────────────────────────
    // Go buildTextBin: u16 LE count, u16 LE offsets, then u16 LE
    // byte-length + bytes per text (ordered by controller text id).
    assert_eq!(
        a.text_bin,
        vec![
            4, 0, // count = 4
            0, 0, // "Hello" offset 0
            7, 0, // "IfA"   offset 2+5 = 7
            12, 0, // "IfB"   offset 7+5 = 12
            17, 0, // "Secret" offset 12+5 = 17
            5, 0, b'H', b'e', b'l', b'l', b'o', //
            3, 0, b'I', b'f', b'A', //
            3, 0, b'I', b'f', b'B', //
            6, 0, b'S', b'e', b'c', b'r', b'e', b't', //
        ]
    );

    // ── hpp files ───────────────────────────────────────────────────
    assert_eq!(
        a.flags_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
         uint16_t flag_A = 1;\n\
         uint16_t flag_f1 = 0;\n\
         uint16_t flag_f2 = 2;\n"
    );
    assert_eq!(
        a.locations_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n"
    );
    assert_eq!(
        a.flag_bit_array_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\
         \nextern uint8_t FLAG_BIT_ARRAY[1];\n"
    );
    assert_eq!(
        a.flag_bit_array_cpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\
         \nuint8_t FLAG_BIT_ARRAY[1] = {0};\n"
    );
    assert_eq!(
        a.scripts_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
         // msg {Hello}; setflag flag_f1\n\
         uint8_t blob0[] = { 0,0,0,6,0,0 };\n\
         // if flag_A then msg {IfA} else msg {IfB} endif; tpif 7 8 9 10; readflag flag_f2; smsg 3 4 {Secret}\n\
         uint8_t blob32[] = { 5,0,0,1,0,3,0,0,1,0,0,2,4,0,7,0,8,0,9,0,10,8,0,2,2,0,3,0,4,0,3 };\n\
         // tmsg 10 20 {Hello}; unsetflag flag_f1; tp 5 6\n\
         uint8_t blob33[] = { 1,0,10,0,20,0,0,7,0,0,3,0,5,0,6 };\n"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 2. Empty map: Go BuildMap on a 256×256 zero map.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn map_pipeline_empty_matches_go_bytes() {
    let a = build_map(&empty_map()).expect("build_map");

    assert_eq!(a.raw_map.len(), 131072);
    assert!(a.raw_map.iter().all(|&b| b == 0));

    assert_eq!(a.scripts_bin.len(), 262144);
    assert!(a.scripts_bin.iter().all(|&b| b == 0));

    // No texts → count 0 only.
    assert_eq!(a.text_bin, vec![0, 0]);

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
         \x20   GiveItem = 9,\n\
         \x20   End = 255,\n\
         };\n"
    );
    assert_eq!(
        a.flags_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n"
    );
    assert_eq!(
        a.locations_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n"
    );
    assert_eq!(
        a.scripts_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n"
    );
    // Zero flags → 0-byte bit array.
    assert_eq!(
        a.flag_bit_array_hpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\
         \nextern uint8_t FLAG_BIT_ARRAY[0];\n"
    );
    assert_eq!(
        a.flag_bit_array_cpp,
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\
         \nuint8_t FLAG_BIT_ARRAY[0] = {0};\n"
    );
}

// ─────────────────────────────────────────────────────────────────────
// 2b. Map with an NPC + script object sharing a chunk: order parity
// (NPC bytes always precede script-object bytes, Go mapbuilder.go).
// ─────────────────────────────────────────────────────────────────────

#[test]
fn map_pipeline_npc_before_script_object_in_chunk() {
    let mut m = empty_map();
    m.npcs = vec![NPCObject {
        id: 1,
        x: 0,
        y: 0,
        sprite_id: 4,
        script: "msg {Hi}".to_string(),
    }];
    m.scripts = vec![ScriptObject {
        id: 2,
        x: 0,
        y: 0,
        script: "setflag flag_door".to_string(),
    }];
    let a = build_map(&m).expect("build_map");
    assert_eq!(&a.scripts_bin[..7], &[0, 0, 0, 6, 0, 0, 0xFF]);
}

// ─────────────────────────────────────────────────────────────────────
// 3. Creatures + moves pipeline parity (creatures.go / moves.go).
// ─────────────────────────────────────────────────────────────────────

#[test]
fn creatures_pipeline_matches_go_bytes() {
    let creatures = load_creatures();
    let (bin, hpp) = build_creatures(&creatures).expect("build_creatures");
    // Go reference (creatures_test.go + fixtures): SquibbleSnail then NewOne.
    // Records are 6 bytes: id, type1, type2, evo, hp seed, folded attack mask
    // (Go type encoding: none = 8).
    assert_eq!(
        bin,
        vec![
            0, 2, 8, 8, 3, 2, // SquibbleSnail (water mask)
            99, 8, 8, 1, 1, 1, // NewOne (spirit mask)
        ]
    );
    assert!(hpp.contains("uint8_t creatureData[]"));
    assert!(hpp.contains("{0, 2, 8, 8, 3, 2},"));
}

// ─────────────────────────────────────────────────────────────────────
// 3b. Bytecode-level parity: parse → ToBytecode vs Go Command2Bytes for
// the full command set (integration-level re-check; unit-level cases in
// script/mod.rs). Uses the same script shapes as the kitchen-sink map.
// ─────────────────────────────────────────────────────────────────────

const FULL_SCRIPT: &str = "msg {Hello}; setflag flag_f1; tmsg 10 20 {Hello}; \
     unsetflag flag_f1; tp 5 6; if flag_A then msg {IfA} else msg {IfB} endif; \
     tpif 7 8 9 10; readflag flag_f2; smsg 3 4 {Secret}";

/// Concatenation of Go `Command2Bytes` outputs for `FULL_SCRIPT` (ids as
/// assigned in parse order — text Hello=0, IfA=1, IfB=2, Secret=3;
/// flags flag_f1=0, flag_A=1, flag_f2=2).
const FULL_SCRIPT_GO_BYTES: &[u8] = &[
    0, 0, 0, // msg {Hello}
    6, 0, 0, // setflag flag_f1
    1, 0, 10, 0, 20, 0, 0, // tmsg 10 20 {Hello}
    7, 0, 0, // unsetflag flag_f1
    3, 0, 5, 0, 6, // tp 5 6
    5, 0, 0, 1, 0, 3, 0, 0, 1, 0, 0, 2, // if flag_A then msg {IfA} else msg {IfB} endif
    4, 0, 7, 0, 8, 0, 9, 0, 10, // tpif 7 8 9 10
    8, 0, 2, // readflag flag_f2
    2, 0, 3, 0, 4, 0, 3, // smsg 3 4 {Secret}
];

#[test]
fn script_to_bytecode_matches_go_command2bytes() {
    let locations: HashMap<String, (u16, u16)> = HashMap::new();
    let mut ctrl = Controller::new();
    let cmds = script::parse(FULL_SCRIPT, &locations, &mut ctrl).expect("parse");
    let mut blob = Vec::new();
    for cmd in &cmds {
        blob.extend_from_slice(&cmd.to_bytes());
    }
    assert_eq!(blob, FULL_SCRIPT_GO_BYTES);
}

// ─────────────────────────────────────────────────────────────────────
// 3c. GiveItem (opcode 9, contract revision 2026-10-12): pinned ABI bytes,
//     unchanged old streams and branch skip offsets.
// ─────────────────────────────────────────────────────────────────────

/// Pinned GiveItem stream: opcode 9 then the three operand bytes
/// `kind,id,count`. Held as a literal (not an encoder round-trip) so the test
/// pins the firmware ABI rather than mirroring the writer.
const GIVE_ITEM_GO_BYTES: &[u8] = &[9, 0, 23, 99];

#[test]
fn giveitem_matches_pinned_stream() {
    let locations: HashMap<String, (u16, u16)> = HashMap::new();
    let mut ctrl = Controller::new();
    let cmds = script::parse("giveitem 0 23 99", &locations, &mut ctrl).expect("parse");
    let mut blob = Vec::new();
    for cmd in &cmds {
        blob.extend_from_slice(&cmd.to_bytes());
    }
    assert_eq!(blob, GIVE_ITEM_GO_BYTES);
    assert_eq!(blob.len(), 4, "GiveItem is opcode + three operand bytes");
}

#[test]
fn giveitem_appended_keeps_old_stream_prefix() {
    let locations: HashMap<String, (u16, u16)> = HashMap::new();
    let mut ctrl = Controller::new();
    let src = format!("{FULL_SCRIPT}; giveitem 1 7 99");
    let cmds = script::parse(&src, &locations, &mut ctrl).expect("parse");
    let mut blob = Vec::new();
    for cmd in &cmds {
        blob.extend_from_slice(&cmd.to_bytes());
    }
    assert_eq!(&blob[..FULL_SCRIPT_GO_BYTES.len()], FULL_SCRIPT_GO_BYTES);
    assert_eq!(&blob[FULL_SCRIPT_GO_BYTES.len()..], &[9, 1, 7, 99]);
}

#[test]
fn giveitem_branch_skip_offset_counts_four_bytes() {
    let locations: HashMap<String, (u16, u16)> = HashMap::new();
    let mut ctrl = Controller::new();
    let cmds =
        script::parse("if flag_A then giveitem 0 1 5 endif", &locations, &mut ctrl).expect("parse");
    // [If][0 flag=0][Then][then-len=4][9 0 1 5]
    assert_eq!(cmds[0].to_bytes(), vec![5, 0, 0, 0, 1, 4, 9, 0, 1, 5]);
}

// ─────────────────────────────────────────────────────────────────────
// 4. Tiled interchange end-to-end on the original creaturevm-rust
// fixture (tests/world_map.json): import → export → reimport.
// ─────────────────────────────────────────────────────────────────────

#[test]
fn tiled_world_map_fixture_roundtrips() {
    let original = tiled::import(&read_fixture("tests/world_map.json")).expect("import");

    // The creaturevm-rust fixture is a full Tiled map: 3 script objects,
    // 2 locations, map tilelayer present.
    assert_eq!(original.width, 256);
    assert_eq!(original.height, 256);
    assert_eq!(original.tiles.len(), 256);
    assert_eq!(original.tiles[0].len(), 256);
    assert_eq!(original.scripts.len(), 3);
    assert_eq!(original.scripts[0].script, "tp @test_teleport;");
    assert_eq!(original.npcs.len(), 2);

    // export(import(x)) → reimport must be an equivalent GameMap.
    let exported = tiled::export(&original);
    let reimported = tiled::import(&exported).expect("reimport");
    assert_eq!(reimported, original);
}

/// Same round-trip contract holds for the expanded 5-script fixture used
/// by crate::tiled's unit tests and the builder fixtures.
#[test]
fn tiled_testdata_world_map_fixture_roundtrips_integration() {
    let original = tiled::import(&read_fixture("testdata/world_map.json")).expect("import");
    let reimported = tiled::import(&tiled::export(&original)).expect("reimport");
    assert_eq!(reimported, original);
}
