use creaturevm::fxpack::spec::{CArraySpec, LayoutSpec, Source, SymbolSpec};
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
            .map(|entry| entry.name.as_deref())
            .collect::<Vec<_>>(),
        [Some("type_table"), Some("tiles"), Some("creatureData")]
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
            .map(|entry| (entry.namespace.as_deref(), entry.name.as_deref()))
            .collect::<Vec<_>>(),
        [
            (Some("MenuFXData"), Some("special")),
            (None, Some("special")),
            (Some("MenuFXData::Nested"), Some("special")),
            (Some("OtherFXData::Nested"), Some("special")),
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
fn nested_namespace_paths_preserve_order() {
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
fn typed_symbol_source_paths_and_names_resolve() {
    let path = fixture("symbol.toml");
    let base = path.parent().unwrap();
    let spec = LayoutSpec::load(&path).unwrap();
    assert!(matches!(
        &spec.entries[0].source,
        Source::Symbol(SymbolSpec { path: source, symbol })
            if source == &base.join("../symbol/data.txt") && symbol == "refs"
    ));
}

#[test]
fn expand_source_parses_without_an_entry_name() {
    let path = fixture("expand.toml");
    let base = path.parent().unwrap();
    let spec = LayoutSpec::load(&path).unwrap();

    assert_eq!(spec.entries.len(), 1);
    assert_eq!(spec.entries[0].name, None);
    assert!(matches!(
        &spec.entries[0].source,
        Source::Expand { path: source, exclude: Some(exclude) }
            if source == &base.join("expand.txt") && exclude == &vec!["second".to_owned()]
    ));
}

#[test]
fn concrete_sources_require_names_but_expand_rejects_name_namespace_and_align() {
    for (fixture_name, expected) in [
        ("missing_entry_name.toml", "name is required"),
        ("expand_with_name.toml", "must not name"),
        ("expand_with_namespace.toml", "must not set namespace"),
        ("expand_with_align.toml", "must not set align"),
    ] {
        let error = load_error(fixture_name);
        assert!(error.contains(expected), "{fixture_name}: {error}");
    }
}

#[test]
fn expand_excludes_must_name_declarations() {
    let error = load_error("expand_unknown_exclude.toml");
    assert!(
        error.contains("does not declare excluded name \"unknown\""),
        "{error}"
    );
}

#[test]
fn expand_requires_declarations_and_closed_namespaces() {
    let empty_error = load_error("expand_empty.toml");
    assert!(empty_error.contains("zero declarations"), "{empty_error}");

    let namespace_error = load_error("expand_unterminated.toml");
    assert!(
        namespace_error.contains("namespace \"Open\" is not closed"),
        "{namespace_error}"
    );
}

#[test]
fn expand_declarations_cannot_duplicate_concrete_entry_symbols() {
    let error = load_error("expand_duplicate.toml");
    assert!(error.contains("duplicate entry name \"first\""), "{error}");
}

#[test]
fn expand_source_preserves_deny_unknown_fields() {
    let error = load_error("expand_unknown_field.toml");
    assert!(error.contains("unexpected"), "{error}");
    assert!(error.contains("unknown"), "{error}");
}
