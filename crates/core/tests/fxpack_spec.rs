use creaturevm::fxpack::spec::{CArraySpec, LayoutSpec, Source};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/fxpack/spec")
        .join(name)
}

fn load_error(name: &str) -> String {
    format!("{:#}", LayoutSpec::load(&fixture(name)).unwrap_err())
}

#[test]
fn valid_layout_preserves_entry_order_and_save_block() {
    let spec = LayoutSpec::load(&fixture("valid.toml")).unwrap();

    assert_eq!(
        spec.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["type_table", "tiles", "creatureData"]
    );
    assert_eq!(spec.builder_keys(), ["creature_data"]);
    let save = spec.save.as_ref().unwrap();
    assert_eq!(save.sectors, 2);
    assert_eq!(
        save.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["save_main", "save_log"]
    );
}

#[test]
fn relative_source_paths_resolve_against_spec_directory() {
    let path = fixture("valid.toml");
    let base = path.parent().unwrap();
    let spec = LayoutSpec::load(&path).unwrap();

    assert!(matches!(
        &spec.entries[0].source,
        Source::Raw(source) if source == &base.join("generated/typetable.bin")
    ));
    assert!(matches!(
        &spec.entries[1].source,
        Source::Image { path: source, .. } if source == &base.join("images/tiles.png")
    ));
}

#[test]
fn carray_source_paths_resolve_against_spec_directory() {
    let path = fixture("carray.toml");
    let base = path.parent().unwrap();
    let spec = LayoutSpec::load(&path).unwrap();

    assert!(matches!(
        &spec.entries[0].source,
        Source::CArray(CArraySpec::Simple(source)) if source == &base.join("carray.txt")
    ));
}

#[test]
fn filtered_carray_source_paths_and_symbols_resolve_against_spec_directory() {
    let path = fixture("filtered_carray.toml");
    let base = path.parent().unwrap();
    let spec = LayoutSpec::load(&path).unwrap();

    assert!(matches!(
        &spec.entries[0].source,
        Source::CArray(CArraySpec::Filtered { path: source, symbol })
            if source == &base.join("carray.txt") && symbol == "selected"
    ));
}

#[test]
fn duplicate_entry_name_is_rejected_with_qualified_name() {
    assert!(
        load_error("duplicate_name.toml")
            .contains("duplicate entry name \"MenuFXData::duplicate\"")
    );
}

#[test]
fn duplicate_names_in_distinct_namespace_paths_are_accepted_in_order() {
    let spec = LayoutSpec::load(&fixture("same_name_different_namespaces.toml")).unwrap();

    assert_eq!(
        spec.entries
            .iter()
            .map(|entry| (entry.namespace.as_deref(), entry.name.as_str()))
            .collect::<Vec<_>>(),
        [
            (Some("MenuFXData"), "special"),
            (None, "special"),
            (Some("MenuFXData::Nested"), "special"),
            (Some("OtherFXData::Nested"), "special"),
        ]
    );
}

#[test]
fn invalid_identifier_is_rejected() {
    assert!(load_error("invalid_identifier.toml").contains("not-valid"));
}

#[test]
fn unsupported_image_shades_are_rejected() {
    assert!(load_error("invalid_shades.toml").contains("shades 5"));
}

#[test]
fn non_power_of_two_align_is_rejected() {
    assert!(load_error("invalid_align.toml").contains("power of two"));
}

#[test]
fn missing_source_files_name_resolved_path() {
    let raw_error = load_error("missing_raw.toml");
    assert!(raw_error.contains("missing.bin"));
    assert!(raw_error.contains("does not exist"));

    let image_error = load_error("missing_image.toml");
    assert!(image_error.contains("missing.png"));
    assert!(image_error.contains("does not exist"));

    let carray_error = load_error("missing_carray.toml");
    assert!(carray_error.contains("missing.txt"));
    assert!(carray_error.contains("does not exist"));
}

#[test]
fn invalid_save_offsets_are_rejected() {
    assert!(load_error("misaligned_save.toml").contains("aligned"));
    assert!(load_error("descending_save.toml").contains("ascending"));
}

#[test]
fn unknown_source_is_rejected_by_deserialization() {
    let error = load_error("unknown_source.toml");
    assert!(error.contains("unknown"));
}

#[test]
fn nested_namespace_paths_preserve_order_and_legacy_symbols() {
    let spec = LayoutSpec::load(&fixture("nested_namespace.toml")).unwrap();

    assert_eq!(
        spec.entries
            .iter()
            .map(|entry| entry.namespace.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("Teams::StockCreatures"),
            Some("Teams::Names"),
            Some("Teams"),
        ]
    );
    assert_eq!(spec.legacy_symbols(), ["Teams::teamList"]);
}

#[test]
fn namespace_paths_reject_empty_or_invalid_components() {
    for fixture_name in [
        "invalid_namespace_empty_component.toml",
        "invalid_namespace_component.toml",
    ] {
        let error = load_error(fixture_name);
        assert!(error.contains("namespace path"), "{fixture_name}: {error}");
        assert!(
            error.contains("components separated by ::"),
            "{fixture_name}: {error}"
        );
    }
}

#[test]
fn namespace_qualified_legacy_source_is_exposed() {
    let spec = LayoutSpec::load(&fixture("legacy.toml")).unwrap();
    assert!(matches!(
        &spec.entries[0].source,
        Source::Legacy(symbol) if symbol == "EventData::eventTable"
    ));
    assert_eq!(spec.legacy_symbols(), ["EventData::eventTable"]);
}
