use creaturevm::arena_demo::{self, Catalog, Member, OpponentTeam, PlayerTeam};
use std::path::{Path, PathBuf};

fn temp_dir() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("cgfx-arena-{stamp}"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn make_font(path: &Path, cols: u32) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), cols * 5, 6);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    let mut pixels = vec![255; (cols * 5 * 6 * 4) as usize];
    for code in 32..cols.min(127) {
        let offset = (code * 5 * 4) as usize;
        pixels[offset..offset + 3].copy_from_slice(&[0, 0, 0]);
    }
    writer.write_image_data(&pixels).unwrap();
}

fn catalog(font: PathBuf) -> Catalog {
    Catalog {
        label_font: font,
        players: vec![
            PlayerTeam {
                key: "starter".into(),
                label: "Starter".into(),
                members: [
                    Member {
                        species: 0,
                        level: 31,
                        move_ids: [10, 32, 32, 32],
                    },
                    Member {
                        species: 3,
                        level: 30,
                        move_ids: [11, 32, 32, 32],
                    },
                    Member {
                        species: 6,
                        level: 29,
                        move_ids: [12, 32, 32, 32],
                    },
                ],
            },
            PlayerTeam {
                key: "reserve".into(),
                label: "Reserve".into(),
                members: [
                    Member {
                        species: 9,
                        level: 20,
                        move_ids: [13, 32, 32, 32],
                    },
                    Member {
                        species: 12,
                        level: 21,
                        move_ids: [14, 32, 32, 32],
                    },
                    Member {
                        species: 15,
                        level: 22,
                        move_ids: [15, 32, 32, 32],
                    },
                ],
            },
        ],
        opponents: vec![
            OpponentTeam {
                key: "rival".into(),
                label: "Rival".into(),
                trainer_id: 4,
                species: [0, 3, 6],
            },
            OpponentTeam {
                key: "champion".into(),
                label: "Champion".into(),
                trainer_id: 7,
                species: [9, 12, 15],
            },
        ],
    }
}

fn array_values(text: &str, declaration: &str) -> Vec<u8> {
    let start = text
        .find(declaration)
        .unwrap_or_else(|| panic!("missing {declaration}"));
    let body = &text[start..];
    let body = body
        .split_once("{\n")
        .unwrap()
        .1
        .split_once("};")
        .unwrap()
        .0;
    body.split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap())
        .collect()
}

#[test]
fn emitter_preserves_catalog_order_geometry_ids_and_fixture_data() {
    let root = temp_dir();
    let font = root.join("font.png");
    make_font(&font, 128);
    let generated = root.join("generated");
    let fixtures = root.join("fixtures");
    let catalog = catalog(font);
    arena_demo::emit(&catalog, &generated, Some(&fixtures)).unwrap();

    let descriptor = std::fs::read_to_string(generated.join("arena_demo.txt")).unwrap();
    let ids = std::fs::read_to_string(generated.join("arena_demo_ids.hpp")).unwrap();
    let fixture = std::fs::read_to_string(fixtures.join("arena_demo_data.hpp")).unwrap();

    assert_eq!(
        array_values(&descriptor, "uint8_t playerMembers[]"),
        vec![
            0, 31, 10, 32, 32, 32, 3, 30, 11, 32, 32, 32, 6, 29, 12, 32, 32, 32, 9, 20, 13, 32, 32,
            32, 12, 21, 14, 32, 32, 32, 15, 22, 15, 32, 32, 32,
        ]
    );
    assert_eq!(
        array_values(&descriptor, "uint8_t opponentTrainerIds[]"),
        [4, 7]
    );
    assert_eq!(
        array_values(&descriptor, "uint8_t opponentSpecies[]"),
        [0, 3, 6, 9, 12, 15]
    );

    let player_labels = descriptor.find("uint24_t playerLabels[]").unwrap();
    let opponent_labels = descriptor.find("uint24_t opponentLabels[]").unwrap();
    assert!(
        descriptor.find("uint8_t player_starter[]").unwrap()
            < descriptor.find("uint8_t player_reserve[]").unwrap()
    );
    assert!(
        descriptor.find("uint8_t player_reserve[]").unwrap()
            < descriptor.find("uint8_t opponent_rival[]").unwrap()
    );
    assert!(
        descriptor.find("uint8_t opponent_rival[]").unwrap()
            < descriptor.find("uint8_t opponent_champion[]").unwrap()
    );
    assert!(descriptor.find("uint8_t opponent_champion[]").unwrap() < player_labels);
    assert!(player_labels < opponent_labels);
    assert!(descriptor.contains("uint24_t playerLabels[] = { player_starter, player_reserve }"));
    assert!(
        descriptor.contains("uint24_t opponentLabels[] = { opponent_rival, opponent_champion }")
    );
    assert_eq!(
        array_values(&descriptor, "uint8_t playerLabelWidths[]"),
        [35, 35]
    );
    assert_eq!(
        array_values(&descriptor, "uint8_t opponentLabelWidths[]"),
        [25, 40]
    );
    assert_eq!(
        array_values(&descriptor, "uint8_t player_starter[]").len(),
        70
    );
    assert_eq!(
        array_values(&descriptor, "uint8_t opponent_champion[]").len(),
        80
    );

    assert!(ids.contains("constexpr uint8_t playerCount = 2;"));
    assert!(ids.contains("constexpr uint8_t opponentCount = 2;"));
    assert!(ids.contains("constexpr uint8_t MemberBytes = 6;"));
    assert!(ids.contains("constexpr uint8_t PlayerBytes = 18;"));
    assert!(ids.contains("constexpr uint8_t player_starter = 0;"));
    assert!(ids.contains("constexpr uint8_t player_reserve = 1;"));
    assert!(ids.contains("constexpr uint8_t opponent_rival = 0;"));
    assert!(ids.contains("constexpr uint8_t opponent_champion = 1;"));
    assert!(ids.contains("constexpr uint8_t trainer_rival = 4;"));
    assert!(ids.contains("constexpr uint8_t trainer_champion = 7;"));
    assert!(fixture.contains("player_starter[] = { 0, 31, 10, 32, 32, 32, 3, 30"));
    assert!(fixture.contains("player_reserve[] = { 9, 20, 13, 32, 32, 32"));
    assert!(fixture.contains("trainer_champion = 7;"));
    assert!(fixture.contains("opponent_champion[] = { 9, 12, 15 }"));

    let descriptor_before = descriptor;
    let ids_before = ids;
    let fixture_before = fixture;
    arena_demo::emit(&catalog, &generated, Some(&fixtures)).unwrap();
    assert_eq!(
        descriptor_before,
        std::fs::read_to_string(generated.join("arena_demo.txt")).unwrap()
    );
    assert_eq!(
        ids_before,
        std::fs::read_to_string(generated.join("arena_demo_ids.hpp")).unwrap()
    );
    assert_eq!(
        fixture_before,
        std::fs::read_to_string(fixtures.join("arena_demo_data.hpp")).unwrap()
    );
}

#[test]
fn emitter_rejects_unmapped_label_glyphs() {
    let root = temp_dir();
    let font = root.join("font.png");
    make_font(&font, 40);
    let error = arena_demo::emit(&catalog(font), &root.join("generated"), None).unwrap_err();
    assert!(error.to_string().contains("unmapped character"), "{error}");
}
