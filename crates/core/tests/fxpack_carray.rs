use creaturevm::fxpack::carray::{
    enumerate_declarations, extract_symbol_bytes, parse_carray_bytes, parse_symbol_bytes, CType,
    Declaration,
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
    assert!(parse_carray_bytes(symbolic)
        .unwrap_err()
        .to_string()
        .contains("unsupported symbolic value"));

    let out_of_range = "uint8_t bytes[] = { 256 };";
    assert!(parse_carray_bytes(out_of_range)
        .unwrap_err()
        .to_string()
        .contains("out of byte range"));
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

fn assert_declaration(
    declarations: &[Declaration],
    index: usize,
    name: &str,
    namespace: Option<&str>,
    ctype: CType,
    line: u32,
) {
    let declaration = &declarations[index];
    assert_eq!(declaration.name, name, "index {index}");
    assert_eq!(declaration.namespace.as_deref(), namespace, "index {index}");
    assert_eq!(declaration.ctype, ctype, "index {index}");
    assert_eq!(declaration.line, line, "index {index}");
    assert_eq!(
        declaration.bytes.is_some(),
        ctype == CType::U8,
        "index {index}"
    );
}

fn assert_source_order(declarations: &[Declaration]) {
    assert!(
        declarations
            .windows(2)
            .all(|pair| pair[0].line < pair[1].line),
        "declarations must remain in source order"
    );
}

#[test]
fn enumerates_real_generated_sprite_declarations() {
    let declarations = declarations("fxdata/generated/Sprites.txt");

    assert_eq!(declarations.len(), 108);
    assert_source_order(&declarations);
    assert_declaration(&declarations, 0, "amove20", None, CType::U8, 1);
    assert_declaration(&declarations, 52, "plant", None, CType::U8, 963);
    assert_declaration(
        &declarations,
        104,
        "CreatureNames",
        Some("CreatureNames"),
        CType::U24,
        1998,
    );
    assert_declaration(
        &declarations,
        107,
        "EffectStrings",
        Some("EffectStrings"),
        CType::U24,
        2007,
    );
}

#[test]
fn enumerates_real_move_declarations() {
    let declarations = declarations("fxdata/generated/moves.txt");

    assert_eq!(declarations.len(), 45);
    assert_source_order(&declarations);
    assert_declaration(&declarations, 0, "move0", Some("MoveData"), CType::U8, 2);
    assert_declaration(&declarations, 22, "move22", Some("MoveData"), CType::U8, 24);
    assert_declaration(
        &declarations,
        44,
        "moveNames",
        Some("MoveData"),
        CType::U24,
        47,
    );
}

#[test]
fn enumerates_real_menu_declarations() {
    let declarations = declarations("fxdata/data/menu.txt");

    assert_eq!(declarations.len(), 37);
    assert_source_order(&declarations);
    assert_declaration(
        &declarations,
        0,
        "attackText",
        Some("MenuFXData"),
        CType::U8,
        2,
    );
    assert_declaration(&declarations, 18, "NONE", Some("MenuFXData"), CType::U8, 20);
    assert_declaration(
        &declarations,
        36,
        "gather",
        Some("MenuFXData"),
        CType::U8,
        38,
    );
}

#[test]
fn enumerates_real_team_declarations() {
    let declarations = declarations("fxdata/generated/teams.txt");

    assert_eq!(declarations.len(), 35);
    assert_source_order(&declarations);
    assert_declaration(
        &declarations,
        0,
        "null",
        Some("Teams::StockCreatures"),
        CType::U8,
        3,
    );
    assert_declaration(
        &declarations,
        32,
        "ardu",
        Some("Teams::StockCreatures"),
        CType::U8,
        35,
    );
    assert_declaration(&declarations, 34, "teamList", Some("Teams"), CType::U24, 43);
}

#[test]
fn enumerates_real_legacy_sprite_declarations() {
    let declarations = declarations("fxdata/Sprites.txt");

    assert_eq!(declarations.len(), 22);
    assert_source_order(&declarations);
    assert_declaration(&declarations, 0, "numbersblack", None, CType::U8, 1);
    assert_declaration(&declarations, 10, "moveInfo", None, CType::U8, 5857);
    assert_declaration(&declarations, 21, "letters", None, CType::U8, 15258);
}

#[test]
fn enumerates_real_opponent_declarations() {
    let declarations = declarations("fxdata/generated/opponents.txt");

    assert_eq!(declarations.len(), 20);
    assert_source_order(&declarations);
    assert_declaration(&declarations, 0, "opponent_seeds", None, CType::U8, 1);
    assert_declaration(&declarations, 10, "opponent_seed_10", None, CType::U8, 61);
    assert_declaration(&declarations, 19, "opts", None, CType::U24, 115);
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
