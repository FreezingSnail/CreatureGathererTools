use creaturevm::fxpack::carray::{
    CType, Declaration, enumerate_declarations, extract_symbol_bytes, parse_carray_bytes,
    parse_symbol_bytes,
};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/fxpack/spec")
        .join(name)
}

#[test]
fn extracts_decimal_values_from_all_uint8_initializers() {
    let source = r#"
        uint8_t first[] = { 1, 2, 3 };
        namespace Data { uint8_t second = { 4, 5 }; }
    "#;

    assert_eq!(parse_carray_bytes(source).unwrap(), [1, 2, 3, 4, 5]);
}

#[test]
fn extracts_hexadecimal_values() {
    let source = "uint8_t opponent_seeds[] = { 0x00, 0x1F, 0X80, 0xFF };";

    assert_eq!(
        parse_carray_bytes(source).unwrap(),
        [0x00, 0x1F, 0x80, 0xFF]
    );
}

#[test]
fn extracts_string_bytes_and_escapes() {
    let source = r#"
        uint8_t move0[] = "smite\0";
        uint8_t control[] = { "\"\n\r\t\x21" };
    "#;

    assert_eq!(parse_carray_bytes(source).unwrap(), b"smite\0\"\n\r\t!");
}

#[test]
fn skips_comments_and_flattens_nested_braces() {
    let source = r#"
        // uint8_t ignored[] = { 99 };
        uint8_t encounter_rates[] = {
            { 1, /* 2, */ 3 },
            // 4,
            { 5, 6 },
        };
    "#;

    assert_eq!(parse_carray_bytes(source).unwrap(), [1, 3, 5, 6]);
}

#[test]
fn ignores_numbers_outside_uint8_initializers_and_uint24_declarations() {
    let source = r#"
        // 99
        constexpr int version = 42;
        uint24_t pointers[] = { { m1e1, m1e1Text }, 0x123456 };
        uint8_t bytes[] = { 7, 8 };
    "#;

    assert_eq!(parse_carray_bytes(source).unwrap(), [7, 8]);
}

#[test]
fn rejects_symbolic_values_and_out_of_range_bytes() {
    let symbolic = "uint8_t bytes[] = { other };";
    assert!(
        parse_carray_bytes(symbolic)
            .unwrap_err()
            .to_string()
            .contains("unsupported symbolic value")
    );

    let out_of_range = "uint8_t bytes[] = { 256 };";
    assert!(
        parse_carray_bytes(out_of_range)
            .unwrap_err()
            .to_string()
            .contains("out of byte range")
    );
}

#[test]
fn extracts_a_named_symbol_from_a_file() {
    assert_eq!(
        extract_symbol_bytes(&fixture("carray.txt"), "selected").unwrap(),
        [4, 5, 6]
    );
}

#[test]
fn skips_symbols_that_are_substrings_of_other_names() {
    let source = r#"
        uint8_t opponent_seeds_extra[] = { 1, 2 };
        uint8_t opponent_seeds[] = { 3, 4 };
    "#;

    assert_eq!(
        parse_symbol_bytes(source, "opponent_seeds").unwrap(),
        [3, 4]
    );
}

#[test]
fn extracts_string_literal_declarations() {
    let source = r#"
        uint8_t ignored[] = { 1 };
        uint8_t move_name[] = "smite\0";
    "#;

    assert_eq!(parse_symbol_bytes(source, "move_name").unwrap(), b"smite\0");
}

#[test]
fn extracts_brace_initializer_declarations() {
    let source = r#"
        uint8_t ignored[] = { 1 };
        uint8_t encounter_rates[] = { { 2, 3 }, 4 };
    "#;

    assert_eq!(
        parse_symbol_bytes(source, "encounter_rates").unwrap(),
        [2, 3, 4]
    );
}

#[test]
fn rejects_missing_symbols() {
    let error = parse_symbol_bytes("uint8_t present[] = { 1 };", "missing")
        .unwrap_err()
        .to_string();

    assert!(error.contains("symbol \"missing\" not found"));
}

#[test]
fn extracts_symbols_inside_namespaces() {
    let source = r#"
        namespace Opponents {
            uint8_t opponent_seeds[] = { 7, 8 };
        }
    "#;

    assert_eq!(
        parse_symbol_bytes(source, "opponent_seeds").unwrap(),
        [7, 8]
    );
}

fn game_source(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../CreatureGathererFX")
        .join(path)
}

fn declarations(path: &str) -> Vec<Declaration> {
    enumerate_declarations(&game_source(path)).unwrap()
}

fn assert_source_order(declarations: &[Declaration]) {
    assert!(
        declarations
            .windows(2)
            .all(|pair| pair[0].line < pair[1].line),
        "declarations must remain in source order"
    );
}

/// `uint8_t` declarations carry decoded bytes; `uint24_t`/`uint32_t`
/// declarations retain source metadata only.
fn assert_bytes_follow_ctype(declarations: &[Declaration]) {
    for declaration in declarations {
        assert_eq!(
            declaration.bytes.is_some(),
            declaration.ctype == CType::U8,
            "{} must carry bytes only for uint8_t",
            declaration.name
        );
        if let Some(bytes) = &declaration.bytes {
            assert!(!bytes.is_empty(), "{} decoded no bytes", declaration.name);
        }
    }
}

fn named<'a>(declarations: &'a [Declaration], name: &str) -> &'a Declaration {
    declarations
        .iter()
        .find(|declaration| declaration.name == name)
        .unwrap_or_else(|| panic!("declaration {name:?} not found"))
}

/// The parser reports the one-based line that names the declaration, so every
/// declaration must land on a real line that contains its name.
fn assert_lines_name_their_declarations(path: &str, declarations: &[Declaration]) {
    let source = std::fs::read_to_string(game_source(path)).expect("game source reads");
    let lines: Vec<&str> = source.lines().collect();
    for declaration in declarations {
        let line = lines
            .get(declaration.line as usize - 1)
            .unwrap_or_else(|| {
                panic!(
                    "{}: reported line {} is outside {}",
                    declaration.name, declaration.line, path
                )
            });
        assert!(
            line.contains(&declaration.name),
            "{}: line {} does not name the declaration: {line:?}",
            declaration.name,
            declaration.line
        );
    }
}

/// Every `uint8_t`/`uint24_t`/`uint32_t` source line is enumerated. This stays
/// self-consistent as the generated row count changes: it asserts the parser's
/// coverage rather than a frozen count.
fn assert_source_declarations_are_exhaustive(path: &str, declarations: &[Declaration]) {
    let source = std::fs::read_to_string(game_source(path)).expect("game source reads");
    let lines: Vec<&str> = source.lines().collect();
    let typed = |line: &str| {
        let trimmed = line.trim_start();
        ["uint8_t", "uint24_t", "uint32_t"].iter().any(|token| {
            trimmed
                .strip_prefix(token)
                .and_then(|rest| rest.chars().next())
                .is_some_and(char::is_whitespace)
        })
    };
    let typed_lines = lines.iter().filter(|line| typed(line)).count();
    let typed_declarations = declarations
        .iter()
        .filter(|declaration| {
            lines
                .get(declaration.line as usize - 1)
                .is_some_and(|line| typed(line))
        })
        .count();
    assert_eq!(
        typed_lines, typed_declarations,
        "{path}: every typed source line must be enumerated"
    );
}

/// Asserts the shared structural invariants for a generated C-array source,
/// then returns the declarations for test-specific semantics.
fn real_declarations(path: &str) -> Vec<Declaration> {
    let declarations = declarations(path);
    assert!(!declarations.is_empty(), "{path} declares nothing");
    assert_source_order(&declarations);
    assert_bytes_follow_ctype(&declarations);
    assert_lines_name_their_declarations(path, &declarations);
    assert_source_declarations_are_exhaustive(path, &declarations);
    declarations
}

#[test]
fn enumerates_real_generated_sprite_declarations() {
    let declarations = real_declarations("fxdata/generated/Sprites.txt");

    // Sprite byte tables sit outside any namespace; the generated source opens
    // with one.
    assert_eq!(declarations[0].namespace, None);
    assert_eq!(declarations[0].ctype, CType::U8);

    // Every trailing typed pointer table lives in a namespace of the same name
    // and carries no decoded bytes.
    assert!(
        declarations.iter().any(|d| d.ctype == CType::U24),
        "generated sprites carry typed pointer tables"
    );
    for declaration in declarations.iter().filter(|d| d.ctype == CType::U24) {
        assert_eq!(
            declaration.namespace.as_deref(),
            Some(declaration.name.as_str()),
            "typed table {} must sit in its own namespace",
            declaration.name
        );
    }
    assert_eq!(declarations.last().unwrap().ctype, CType::U24);

    let creature_names = named(&declarations, "CreatureNames");
    assert_eq!(creature_names.namespace.as_deref(), Some("CreatureNames"));
    assert_eq!(creature_names.ctype, CType::U24);
    assert_eq!(creature_names.bytes, None);
}

#[test]
fn enumerates_real_attack_mask_declarations() {
    let declarations = real_declarations("fxdata/generated/attackMask.txt");

    assert_eq!(declarations.len(), 1, "one folded attack-mask table");
    let mask = named(&declarations, "attackMask");
    assert_eq!(mask.namespace.as_deref(), Some("AttackMasks"));
    assert_eq!(mask.ctype, CType::U8);
    assert!(mask.bytes.as_ref().is_some_and(|bytes| !bytes.is_empty()));
}

#[test]
fn enumerates_real_menu_declarations() {
    let declarations = real_declarations("fxdata/data/menu.txt");

    assert!(
        declarations
            .iter()
            .all(|d| d.namespace.as_deref() == Some("MenuFXData") && d.ctype == CType::U8),
        "every menu string lives in MenuFXData"
    );
    // C-string tables decode to their content plus a NUL terminator.
    assert_eq!(
        named(&declarations, "NONE").bytes.as_deref(),
        Some(&[0u8][..])
    );
    assert_eq!(
        named(&declarations, "gather").bytes.as_deref(),
        Some(&b"gather\0"[..])
    );
}

#[test]
fn enumerates_real_team_declarations() {
    let declarations = real_declarations("fxdata/generated/teams.txt");

    assert_eq!(
        declarations[0].namespace.as_deref(),
        Some("Teams::StockCreatures")
    );
    // Every stock-creature row is a two-byte {id, attack mask} pair.
    for declaration in declarations
        .iter()
        .filter(|d| d.namespace.as_deref() == Some("Teams::StockCreatures"))
    {
        assert_eq!(declaration.ctype, CType::U8);
        assert_eq!(
            declaration.bytes.as_ref().map(Vec::len),
            Some(2),
            "{} is an {{id, mask}} stock row",
            declaration.name
        );
    }
    let team_list = named(&declarations, "teamList");
    assert_eq!(team_list.namespace.as_deref(), Some("Teams"));
    assert_eq!(team_list.ctype, CType::U24);
    assert_eq!(declarations.last().unwrap().name, "teamList");
}

#[test]
fn enumerates_real_legacy_sprite_declarations() {
    let declarations = real_declarations("fxdata/Sprites.txt");

    assert!(
        declarations
            .iter()
            .all(|d| d.ctype == CType::U8 && d.namespace.is_none()),
        "legacy sprite source is namespace-free uint8_t tables"
    );
}

#[test]
fn enumerates_real_opponent_declarations() {
    let declarations = real_declarations("fxdata/generated/opponents.txt");

    assert!(declarations.iter().all(|d| d.namespace.is_none()));
    // Each seed table is three little-endian {id, level, mask} records.
    for declaration in declarations.iter().filter(|d| d.ctype == CType::U8) {
        assert_eq!(
            declaration.bytes.as_ref().map(Vec::len),
            Some(9),
            "{} is a three-member seed table",
            declaration.name
        );
    }
    let opts = named(&declarations, "opts");
    assert_eq!(opts.ctype, CType::U24);
    assert_eq!(opts.namespace, None);
}

#[test]
fn rejects_unsupported_declarations_with_source_line() {
    let path = fixture("unsupported-carray.txt");
    let error = enumerate_declarations(&path).unwrap_err().to_string();
    assert!(error.contains(&format!(
        "{}:1: unsupported declaration type \"uint16_t\"",
        path.display()
    )));
}

#[test]
fn enumerates_types_and_nested_namespaces() {
    let declarations = enumerate_declarations(&fixture("declarations.txt")).unwrap();

    assert_eq!(
        declarations,
        [
            Declaration {
                name: "bytes".to_owned(),
                namespace: Some("Outer".to_owned()),
                ctype: CType::U8,
                bytes: Some(vec![1, 2]),
                line: 2,
            },
            Declaration {
                name: "refs".to_owned(),
                namespace: Some("Outer::Inner".to_owned()),
                ctype: CType::U24,
                bytes: None,
                line: 4,
            },
            Declaration {
                name: "words".to_owned(),
                namespace: None,
                ctype: CType::U32,
                bytes: None,
                line: 7,
            },
        ]
    );
}

#[test]
fn enumerates_legacy_untyped_symbolic_tables() {
    let declarations = enumerate_declarations(&fixture("legacy-untyped-table.txt")).unwrap();

    assert_eq!(
        declarations,
        [Declaration {
            name: "table".to_owned(),
            namespace: Some("EventData".to_owned()),
            ctype: CType::U24,
            bytes: None,
            line: 2,
        }]
    );
}

#[test]
fn enumerates_real_event_declarations_including_legacy_table() {
    let declarations = real_declarations("fxdata/data/event.txt");

    assert!(
        declarations
            .iter()
            .all(|d| d.namespace.as_deref() == Some("EventData")),
        "every event declaration shares EventData"
    );
    // The legacy untyped table is canonicalized to a uint24_t reference that
    // shares the namespace of the typed tables.
    let legacy = named(&declarations, "m1Table");
    assert_eq!(legacy.ctype, CType::U24);
    assert_eq!(legacy.bytes, None);
    let typed = named(&declarations, "eventTable");
    assert_eq!(typed.ctype, CType::U24);
    assert_eq!(typed.namespace, legacy.namespace);
}

#[test]
fn ignores_legacy_untyped_symbolic_tables_without_closing_namespaces() {
    let source = r#"
        namespace EventData {
            uint8_t before[] = { 1 };
            table = { before };
            uint8_t after[] = { 2 };
        } namespace_end
    "#;

    assert_eq!(parse_carray_bytes(source).unwrap(), [1, 2]);
}

#[test]
fn extracts_scalar_uint8_declarations() {
    assert_eq!(parse_carray_bytes("uint8_t count = 7\n").unwrap(), [7]);
}
