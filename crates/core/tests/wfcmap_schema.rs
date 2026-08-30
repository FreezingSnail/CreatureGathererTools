use creaturevm::model::GameMap;
use creaturevm::wfcmap::{
    Anchor, EdgeSocket, RleError, RuleSpec, Side, SocketKind, SocketLabel, Stamp, StampOrigin,
    WfcPiece, WfcProject, WfcRecipe, decode, encode,
};

fn map_without_wfc() -> GameMap {
    serde_json::from_str(
        r#"{
  "width": 2,
  "height": 1,
  "tiles": [[1, 2]],
  "npcs": [],
  "scripts": []
}"#,
    )
    .expect("legacy map parses")
}

fn piece(id: u32, tiles: &str) -> WfcPiece {
    WfcPiece {
        id,
        name: format!("piece-{id}"),
        tw: 2,
        th: 2,
        tiles: tiles.to_string(),
        weight: 1.0,
        limit: None,
        edge_overrides: Vec::new(),
        tags: vec!["test".to_string()],
    }
}

#[test]
fn legacy_game_map_omits_wfc_and_stamps_after_round_trip() {
    let map = map_without_wfc();
    assert!(map.wfc.is_none());
    assert!(map.stamps.is_empty());

    let value = serde_json::to_value(&map).expect("legacy map serializes");
    let object = value.as_object().expect("map JSON object");
    assert!(!object.contains_key("wfc"));
    assert!(!object.contains_key("stamps"));
}

#[test]
fn populated_wfc_project_and_stamp_round_trip_through_json() {
    let project = WfcProject {
        version: 1,
        slot_w: 2,
        slot_h: 2,
        pieces: vec![WfcPiece {
            edge_overrides: vec![EdgeSocket {
                side: Side::N,
                index: 0,
                label: 7,
            }],
            ..piece(10, "1x4")
        }],
        socket_labels: vec![
            SocketLabel {
                id: 7,
                name: "road".to_string(),
                kind: SocketKind::Symmetric,
            },
            SocketLabel {
                id: 8,
                name: "entry".to_string(),
                kind: SocketKind::Directed { partner: 7 },
            },
        ],
        recipes: vec![WfcRecipe {
            id: 3,
            name: "starter".to_string(),
            piece_ids: vec![10],
            size_w: 2,
            size_h: 2,
            seed: 42,
            anchors: vec![Anchor {
                piece_id: 10,
                sx: 0,
                sy: 0,
            }],
            socket_compat: vec![(7, 8)],
            rules: Vec::new(),
        }],
    };
    project.validate().expect("fixture is valid");
    let stamp = Stamp {
        id: 4,
        name: "saved hunk".to_string(),
        w: 2,
        h: 2,
        tiles: "1x4".to_string(),
        origin: Some(StampOrigin {
            recipe_id: 3,
            seed: 42,
            slot_w: 2,
            slot_h: 2,
        }),
    };
    let mut map = map_without_wfc();
    map.wfc = Some(project);
    map.stamps = vec![stamp];

    let json = serde_json::to_string_pretty(&map).expect("map serializes");
    assert!(json.contains("\"tiles\": \"1x4\""));
    let reloaded: GameMap = serde_json::from_str(&json).expect("map reloads");
    assert_eq!(reloaded, map);
}

#[test]
fn rle_round_trips_required_tile_shapes() {
    let randomish = vec![0, 1, 1, 2, 65535, 2, 2, 2, 9, 0, 65535];
    let cases = [
        Vec::new(),
        vec![37],
        vec![9; 65_536],
        vec![0, 1, 0, 1, 0, 1, 0, 1],
        randomish,
    ];
    for tiles in cases {
        let encoded = encode(&tiles);
        assert!(!encoded.contains('\n'));
        assert_eq!(decode(&encoded, tiles.len()).expect("decode"), tiles);
    }
}

#[test]
fn rle_decode_rejects_invalid_runs_and_lengths() {
    assert!(matches!(decode("6x0", 0), Err(RleError::ZeroCount { .. })));
    assert!(matches!(decode("abc", 1), Err(RleError::Malformed { .. })));
    assert!(matches!(decode("6x", 1), Err(RleError::Malformed { .. })));
    assert!(matches!(decode("6,", 1), Err(RleError::Malformed { .. })));
    assert!(matches!(
        decode("6x2", 1),
        Err(RleError::LengthMismatch {
            got: 2,
            expected: 1
        })
    ));
    assert!(matches!(
        decode("65536", 1),
        Err(RleError::TileOutOfRange { .. })
    ));
    assert_eq!(
        decode(" 6, 7x2 ", 3).expect("whitespace tolerated"),
        vec![6, 7, 7]
    );
}

#[test]
fn validate_reports_all_schema_problem_classes_together() {
    let project = WfcProject {
        version: 1,
        slot_w: 2,
        slot_h: 2,
        pieces: vec![
            WfcPiece {
                id: 1,
                name: "bad".to_string(),
                tw: 3,
                th: 0,
                tiles: "1x2".to_string(),
                weight: 0.0,
                limit: None,
                edge_overrides: vec![EdgeSocket {
                    side: Side::N,
                    index: 2,
                    label: 99,
                }],
                tags: Vec::new(),
            },
            piece(1, "1x4"),
            piece(2, "1x4"),
        ],
        socket_labels: vec![
            SocketLabel {
                id: 5,
                name: "bad directed".to_string(),
                kind: SocketKind::Directed { partner: 99 },
            },
            SocketLabel {
                id: 5,
                name: "duplicate".to_string(),
                kind: SocketKind::Symmetric,
            },
        ],
        recipes: vec![
            WfcRecipe {
                id: 7,
                name: "bad".to_string(),
                piece_ids: vec![99],
                size_w: 129,
                size_h: 1,
                seed: 0,
                anchors: vec![
                    Anchor {
                        piece_id: 77,
                        sx: 0,
                        sy: 0,
                    },
                    Anchor {
                        piece_id: 2,
                        sx: 0,
                        sy: 0,
                    },
                    Anchor {
                        piece_id: 2,
                        sx: 0,
                        sy: 0,
                    },
                    Anchor {
                        piece_id: 2,
                        sx: 129,
                        sy: 0,
                    },
                ],
                socket_compat: Vec::new(),
                rules: Vec::new(),
            },
            WfcRecipe {
                id: 7,
                name: "duplicate".to_string(),
                piece_ids: Vec::new(),
                size_w: 1,
                size_h: 0,
                seed: 0,
                anchors: Vec::new(),
                socket_compat: Vec::new(),
                rules: Vec::new(),
            },
        ],
    };

    let errors = project.validate().expect_err("fixture must be invalid");
    let text = errors.join("\n");
    for required in [
        "duplicate piece id",
        "duplicate socket label id",
        "duplicate recipe id",
        "non-positive weight",
        "zero tile dimensions",
        "not a multiple",
        "invalid tiles",
        "unknown socket label",
        "edge override index is out of range",
        "unknown partner",
        "references unknown piece",
        "invalid slot width",
        "invalid slot height",
        "anchor does not fit",
        "anchors overlap slot",
    ] {
        assert!(text.contains(required), "missing {required:?} in {text}");
    }
    assert!(
        errors.len() >= 15,
        "expected aggregate validation errors: {text}"
    );
}

#[test]
fn recipe_rules_default_when_omitted_and_round_trip_tagged() {
    let recipe: WfcRecipe =
        serde_json::from_str(r#"{"id": 1, "piece_ids": [7], "size_w": 2, "size_h": 2}"#)
            .expect("legacy recipe parses");
    assert!(recipe.rules.is_empty());
    let json = serde_json::to_value(WfcRecipe {
        rules: vec![RuleSpec::Count {
            piece_id: 7,
            min: Some(1),
            max: Some(2),
        }],
        ..recipe
    })
    .expect("rules serialize");
    assert_eq!(json["rules"][0]["kind"], "count");
}

#[test]
fn validate_reports_rule_relationship_errors() {
    let mut project: WfcProject =
        serde_json::from_str(include_str!("testdata/wfc/project.json")).expect("fixture parses");
    project.recipes[0].rules = vec![
        RuleSpec::Count {
            piece_id: 99,
            min: Some(3),
            max: Some(1),
        },
        RuleSpec::Exclude {
            a: 1,
            b: 99,
            radius: 0,
        },
        RuleSpec::Border {
            piece_ids: vec![99],
        },
        RuleSpec::Band {
            piece_id: 1,
            from_row: 3,
            to_row: 1,
        },
        RuleSpec::Band {
            piece_id: 1,
            from_row: 0,
            to_row: 3,
        },
    ];
    let errors = project.validate().expect_err("invalid rules");
    let text = errors.join("\n");
    for expected in [
        "count references unknown piece",
        "min greater than max",
        "radius must be greater",
        "border rule references unknown piece",
        "from_row greater",
        "outside recipe",
    ] {
        assert!(text.contains(expected), "missing {expected} in {text}");
    }
}
