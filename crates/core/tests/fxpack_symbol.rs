use creaturevm::fxpack::symbol::parse_symbol_bytes;
use std::collections::BTreeMap;

fn offsets() -> BTreeMap<String, u32> {
    [
        ("Data::local".to_owned(), 0x001234),
        ("Global::value".to_owned(), 0x00abcd),
    ]
    .into_iter()
    .collect()
}

const FIXTURE: &str = include_str!("testdata/fxpack/symbol/data.txt");

#[test]
fn encodes_typed_symbolic_values_at_declaration_width() {
    assert_eq!(
        parse_symbol_bytes(FIXTURE, "refs", Some("Data"), &offsets()).unwrap(),
        [0x00, 0x12, 0x34, 0x00, 0xab, 0xcd, 0x00, 0x00, 0x20]
    );
    assert_eq!(
        parse_symbol_bytes(FIXTURE, "words", Some("Data"), &offsets()).unwrap(),
        [0x12, 0x34, 0x56, 0x78, 0x00, 0x00, 0x00, 0x02]
    );
    assert_eq!(
        parse_symbol_bytes(FIXTURE, "count", Some("Data"), &offsets()).unwrap(),
        [1]
    );
}

#[test]
fn flattens_nested_initializers_and_rejects_missing_offsets() {
    assert_eq!(
        parse_symbol_bytes(FIXTURE, "nested", Some("Data"), &offsets()).unwrap(),
        [0x00, 0x12, 0x34, 0x00, 0xab, 0xcd]
    );
    let error = parse_symbol_bytes(FIXTURE, "refs", Some("Data"), &BTreeMap::new())
        .unwrap_err()
        .to_string();
    assert!(error.contains("unresolved symbolic offset \"local\""));
}
