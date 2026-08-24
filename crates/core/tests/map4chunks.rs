//! 4-chunk test map fixtures (`test_map_4chunks.json` canonical GameMap,
//! `test_map_4chunks_tiled.json` Tiled interchange).
//!
//! Map is 16x8 tiles = 2x2 chunks of 8x4 (see `crate::model::CHUNK_W` /
//! `CHUNK_H`); chunk indices 0, 1, 32, 33.

use creaturevm::builder;
use creaturevm::config::Config;
use creaturevm::model::{NPCObject, ScriptObject};
use creaturevm::store::Store;
use creaturevm::tiled;

const TD: &str = env!("CARGO_MANIFEST_DIR");
const CANONICAL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/test_map_4chunks.json"
);
const TILED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/test_map_4chunks_tiled.json"
);

fn fixture_cfg() -> Config {
    Config {
        creatures_json: String::new(),
        moves_json: String::new(),
        map_json: CANONICAL.to_string(),
        tileset_png: String::new(),
        map_tileset_png: String::new(),
        npc_tileset_png: String::new(),
        output_dir: format!("{TD}/testdata/builder_out/map4chunks"),
        fixture_dir: None,
        fx_layout: None,
    }
}

fn canonical_map() -> creaturevm::model::GameMap {
    Store::new(&fixture_cfg())
        .load_map()
        .expect("canonical fixture parses")
}

#[test]
fn canonical_map_loads() {
    let m = canonical_map();
    assert_eq!((m.width, m.height), (16, 8));
    assert_eq!(m.tiles.len(), 8);
    assert_eq!(m.tiles[0].len(), 16);
    assert_eq!(m.npcs.len(), 3);
    assert_eq!(m.scripts.len(), 6);
    assert_eq!(m.collision_types.len(), 1);
    assert_eq!(m.encounter_zones.len(), 1);
}

#[test]
fn tiled_interchange_imports() {
    let data = std::fs::read_to_string(TILED).expect("read tiled fixture");
    let m = tiled::import(&data).expect("tiled import");

    assert_eq!((m.width, m.height), (16, 8));
    assert_eq!(m.tiles.len(), 8);
    assert_eq!(m.tiles[0].len(), 16);
    assert_eq!(m.tiles[0][0], 2);
    assert_eq!(m.tiles[7][15], 2);

    // Pixel -> tile: floor(px/16).
    assert_eq!(m.scripts.len(), 6);
    assert_eq!(
        m.scripts[0],
        ScriptObject {
            id: 1,
            x: 5,
            y: 2,
            script: "if flag_door then setflag flag_door_used else setflag flag_door endif".into()
        }
    );
    assert_eq!(
        m.scripts[4],
        ScriptObject {
            id: 5,
            x: 8,
            y: 4,
            script: "msg {Sound the horn?}; readflag flag_door".into()
        }
    );

    // Locations carry their @name in the script field (round-trip design).
    assert_eq!(m.npcs.len(), 3);
    assert_eq!(
        m.npcs[0],
        NPCObject {
            id: 7,
            x: 1,
            y: 1,
            sprite_id: 0,
            script: "@start".into()
        }
    );
    assert_eq!(
        m.npcs[1],
        NPCObject {
            id: 8,
            x: 8,
            y: 4,
            sprite_id: 0,
            script: "@hop".into()
        }
    );
    assert_eq!(
        m.npcs[2],
        NPCObject {
            id: 9,
            x: 14,
            y: 6,
            sprite_id: 0,
            script: "@chest".into()
        }
    );

    // No collision/encounter data in Tiled -> synthesized defaults.
    assert_eq!(m.collision, vec![vec![0u8; 16]; 8]);
    assert_eq!(m.encounters, vec![vec![-1i32; 16]; 8]);
}

#[test]
fn tiled_interchange_roundtrips() {
    let data = std::fs::read_to_string(TILED).expect("read tiled fixture");
    let original = tiled::import(&data).expect("tiled import");
    let reimported = tiled::import(&tiled::export(&original)).expect("reimport");
    assert_eq!(reimported, original);
}

#[test]
fn scripts_land_in_four_chunks() {
    let m = canonical_map();
    let a = builder::build_map(&m).expect("build_map");

    // scripts.bin = 2048 chunks x 128 bytes; chunk index = (y/4)*32 + (x/8).
    const CHUNK_SIZE: usize = 128;
    let nonempty: Vec<usize> = (0..(a.scripts_bin.len() / CHUNK_SIZE))
        .filter(|i| {
            a.scripts_bin[i * CHUNK_SIZE..(i + 1) * CHUNK_SIZE]
                .iter()
                .any(|&b| b != 0)
        })
        .collect();
    assert_eq!(nonempty, vec![0, 1, 32, 33], "expected the 2x2 chunk grid");

    // Chunk 0: first entry is the NPC sign msg at (1,1): opcode 0, text id 0.
    assert_eq!(&a.scripts_bin[0..3], &[0, 0, 0]);
    // Every non-empty chunk ends with an End (0xFF) sentinel.
    for &ci in &nonempty {
        let chunk = &a.scripts_bin[ci * CHUNK_SIZE..(ci + 1) * CHUNK_SIZE];
        let last = chunk
            .iter()
            .rposition(|&b| b != 0)
            .expect("non-empty chunk");
        assert_eq!(chunk[last], 0xFF, "chunk {ci} missing End sentinel");
    }

    // Chunk 1: tmsg opcode (1) + cords (8,0) + text id.
    assert_eq!(a.scripts_bin[CHUNK_SIZE], 0x01);

    // text.bin collects every message referenced by the scripts.
    let text_bin = String::from_utf8_lossy(&a.text_bin);
    assert!(text_bin.contains("Welcome to the test grove"));
    assert!(text_bin.contains("North gate. You are in chunk one."));
    assert!(text_bin.contains("Sound the horn?"));
}
