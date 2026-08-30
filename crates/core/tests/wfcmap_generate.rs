use creaturevm::wfcmap::{
    Anchor, GenerateError, RuleSpec, WfcPiece, WfcProject, compile_recipe, decode, encode, generate,
};

const FIXTURE: &str = include_str!("testdata/wfc/project.json");

fn project() -> WfcProject {
    serde_json::from_str(FIXTURE).expect("WFC fixture parses")
}

#[test]
fn compile_has_stable_symbols_weights_and_valid_rules() {
    let project = project();
    let recipe = &project.recipes[0];
    let compiled = compile_recipe(&project, recipe).expect("compile");
    assert_eq!(compiled.symbols.len(), 2);
    assert_eq!(compiled.symbols[0].piece_id, 1);
    assert_eq!(compiled.symbols[1].piece_id, 2);
    assert_eq!(compiled.symbols[0].tiles, vec![1]);
    assert_eq!(compiled.symbols[1].tiles, vec![2]);
    assert_eq!(compiled.rules.weights(), &[1.0, 1.0]);
    assert!(compiled.rules.validate().is_ok());
}

#[test]
fn alternating_recipe_is_deterministic_and_bakes_slot_grid() {
    let project = project();
    let first = generate(&project, 1, None, Some(7)).expect("generate");
    let second = generate(&project, 1, None, Some(7)).expect("generate");
    assert_eq!(first, second);
    assert_eq!((first.w, first.h), (4, 3));
    assert_eq!((first.slot_w, first.slot_h), (1, 1));
    assert_eq!(first.tiles.len(), 12);
    for y in 0..first.h {
        for x in 0..first.w {
            if x + 1 < first.w {
                assert_ne!(
                    first.tiles[(y * first.w + x) as usize],
                    first.tiles[(y * first.w + x + 1) as usize]
                );
            }
            if y + 1 < first.h {
                assert_ne!(
                    first.tiles[(y * first.w + x) as usize],
                    first.tiles[((y + 1) * first.w + x) as usize]
                );
            }
        }
    }
    assert_eq!(encode(&first.tiles), "1,2,1,2x2,1,2,1x2,2,1,2");
}

#[test]
fn anchored_piece_is_fully_baked_at_anchor_slot() {
    let mut project = project();
    project.pieces[0].tw = 2;
    project.pieces[0].tiles = encode(&[3, 4]);
    project.pieces[0].edge_overrides.clear();
    project.recipes[0].anchors = vec![Anchor {
        piece_id: 1,
        sx: 0,
        sy: 0,
    }];
    let hunk = generate(&project, 1, Some((2, 1)), Some(7)).expect("anchored generate");
    assert_eq!(hunk.tiles, vec![3, 4]);
    assert_eq!(hunk.slots.len(), 2);
}

#[test]
fn anchor_errors_are_typed() {
    let mut project = project();
    project.recipes[0].anchors = vec![Anchor {
        piece_id: 1,
        sx: 4,
        sy: 0,
    }];
    assert!(matches!(
        generate(&project, 1, None, None),
        Err(GenerateError::AnchorOutOfBounds { piece_id: 1, .. })
    ));

    project.recipes[0].anchors = vec![
        Anchor {
            piece_id: 1,
            sx: 0,
            sy: 0,
        },
        Anchor {
            piece_id: 2,
            sx: 0,
            sy: 0,
        },
    ];
    assert!(matches!(
        generate(&project, 1, None, None),
        Err(GenerateError::AnchorOverlap { sx: 0, sy: 0 })
    ));
}

#[test]
fn limit_zero_returns_without_hanging() {
    let mut project = project();
    project.pieces[0].limit = Some(0);
    let error = generate(&project, 1, None, Some(7)).expect_err("limit is impossible");
    assert!(matches!(
        error,
        GenerateError::LimitUnsatisfied {
            piece_id: 1,
            limit: 0,
            attempts: 10
        }
    ));
}

#[test]
fn validation_is_returned_before_solver() {
    let mut project = project();
    project.pieces.push(WfcPiece {
        id: 3,
        name: "bad".into(),
        tw: 0,
        th: 1,
        tiles: String::new(),
        weight: 0.0,
        limit: None,
        edge_overrides: Vec::new(),
        tags: Vec::new(),
    });
    let error = generate(&project, 1, None, None).expect_err("invalid project");
    assert!(matches!(error, GenerateError::Validation(issues) if issues.len() >= 2));
}

#[test]
fn fixture_rle_decoder_accepts_golden_shape() {
    let project = project();
    let hunk = generate(&project, 1, None, Some(7)).expect("generate");
    assert_eq!(
        decode(&encode(&hunk.tiles), hunk.tiles.len()).unwrap(),
        hunk.tiles
    );
}

fn free_project(size_w: u32, size_h: u32) -> WfcProject {
    let mut project = project();
    for piece in &mut project.pieces {
        piece.edge_overrides.clear();
        piece.tiles = "1".to_string();
    }
    project.recipes[0].size_w = size_w;
    project.recipes[0].size_h = size_h;
    project
}

#[test]
fn border_and_band_rules_constrain_only_their_target_cells() {
    let mut border = free_project(4, 3);
    border.recipes[0].rules = vec![RuleSpec::Border { piece_ids: vec![1] }];
    let hunk = generate(&border, 1, None, Some(4)).expect("border rule");
    for (cell, symbol) in hunk.slots.iter().enumerate() {
        let x = cell as u32 % 4;
        let y = cell as u32 / 4;
        if x == 0 || y == 0 || x == 3 || y == 2 {
            assert_eq!(*symbol, 0, "border cell {cell}");
        }
    }

    let mut band = free_project(4, 3);
    band.recipes[0].rules = vec![RuleSpec::Band {
        piece_id: 1,
        from_row: 1,
        to_row: 1,
    }];
    let hunk = generate(&band, 1, None, Some(4)).expect("band rule");
    for (cell, symbol) in hunk.slots.iter().enumerate() {
        if cell as u32 / 4 != 1 {
            assert_ne!(*symbol, 0, "piece one outside band at {cell}");
        }
    }
}

#[test]
fn count_and_exclude_rules_retry_and_name_unsatisfied_rules() {
    let mut count = free_project(4, 3);
    count.recipes[0].rules = vec![RuleSpec::Count {
        piece_id: 1,
        min: Some(13),
        max: None,
    }];
    let error = generate(&count, 1, None, Some(2)).expect_err("impossible minimum");
    assert!(
        matches!(error, GenerateError::RuleUnsatisfied { ref rule, attempts: 10 } if rule.contains("count piece 1"))
    );

    let mut max = free_project(4, 3);
    max.recipes[0].piece_ids = vec![1];
    max.recipes[0].rules = vec![RuleSpec::Count {
        piece_id: 1,
        min: None,
        max: Some(0),
    }];
    let error = generate(&max, 1, None, Some(2)).expect_err("impossible maximum");
    assert!(
        matches!(error, GenerateError::RuleUnsatisfied { ref rule, .. } if rule.contains("max Some(0)"))
    );

    let mut exclude = free_project(2, 1);
    exclude.recipes[0].rules = vec![RuleSpec::Exclude {
        a: 1,
        b: 2,
        radius: 1,
    }];
    exclude.recipes[0].anchors = vec![
        Anchor {
            piece_id: 1,
            sx: 0,
            sy: 0,
        },
        Anchor {
            piece_id: 2,
            sx: 1,
            sy: 0,
        },
    ];
    let error = generate(&exclude, 1, None, Some(2)).expect_err("adjacent exclusion");
    assert!(
        matches!(error, GenerateError::RuleUnsatisfied { ref rule, .. } if rule.contains("exclude pieces 1/2 radius 1"))
    );
}

#[test]
fn active_rules_preserve_determinism_and_band_impossibility_is_typed() {
    let mut project = free_project(4, 3);
    project.recipes[0].rules = vec![RuleSpec::Count {
        piece_id: 1,
        min: Some(0),
        max: Some(4),
    }];
    assert_eq!(
        generate(&project, 1, None, Some(99)).unwrap(),
        generate(&project, 1, None, Some(99)).unwrap()
    );

    project.recipes[0].piece_ids = vec![1];
    project.recipes[0].size_w = 1;
    project.recipes[0].size_h = 2;
    project.recipes[0].rules = vec![RuleSpec::Band {
        piece_id: 1,
        from_row: 0,
        to_row: 0,
    }];
    let error = generate(&project, 1, None, Some(1)).expect_err("band cannot fill row one");
    assert!(
        matches!(error, GenerateError::RuleUnsatisfied { ref rule, .. } if rule.contains("band piece 1"))
    );
}
