use creaturevm::fxpack::carray::{extract_symbol_bytes, parse_carray_bytes, parse_symbol_bytes};
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
