use creaturevm::fxpack::layout::{
    FLASH_PAGES, FieldKind, Layout, LayoutError, PAGE_SIZE, SAVE_SECTOR,
};

#[test]
fn sequential_fields_advance_by_written_length() {
    let mut layout = Layout::new();
    assert_eq!(layout.field("first", FieldKind::Bytes), Ok(0));
    layout.write(&[1, 2, 3]).unwrap();
    assert_eq!(layout.field("second", FieldKind::Bytes), Ok(3));
    layout.write(&[4, 5]).unwrap();
    assert_eq!(layout.pad(8, false), 3);
    assert_eq!(layout.field("third", FieldKind::Bytes), Ok(8));
    assert_eq!(
        layout
            .fields()
            .iter()
            .map(|field| (field.source_len, field.span))
            .collect::<Vec<_>>(),
        vec![(3, 3), (2, 5), (0, 0)]
    );

    let (offsets, _) = layout.finalize().unwrap();
    assert_eq!(offsets.data_len, 8);
}

#[test]
fn padding_respects_alignment_and_force() {
    let mut layout = Layout::new();
    assert_eq!(layout.pad(PAGE_SIZE, false), 0);
    assert_eq!(layout.pad(PAGE_SIZE, true), PAGE_SIZE);

    let (offsets, bytes) = layout.finalize().unwrap();
    assert_eq!(offsets.data_len, PAGE_SIZE);
    assert_eq!(bytes, vec![0xff; PAGE_SIZE as usize]);
}

#[test]
fn namespaced_fields_preserve_namespace_and_unterminated_namespace_fails() {
    let mut layout = Layout::new();
    layout.begin_namespace("Teams::StockCreatures");
    layout.field("hero", FieldKind::Bytes).unwrap();
    assert_eq!(
        layout.fields()[0].namespace.as_deref(),
        Some("Teams::StockCreatures")
    );
    layout.end_namespace().unwrap();

    let mut no_namespace = Layout::new();
    assert_eq!(
        no_namespace.end_namespace(),
        Err(LayoutError::NamespaceNotOpen)
    );

    let mut unterminated = Layout::new();
    unterminated.begin_namespace("sprites");
    assert_eq!(
        unterminated.finalize(),
        Err(LayoutError::UnterminatedNamespace("sprites".to_owned()))
    );
}

#[test]
fn data_only_finalize_pads_to_page_and_reports_top_anchored_page() {
    let mut layout = Layout::new();
    layout.field("data", FieldKind::Bytes).unwrap();
    layout.write(&[0x42]).unwrap();

    let (offsets, bytes) = layout.finalize().unwrap();
    assert_eq!(offsets.data_len, 1);
    assert_eq!(offsets.data_len_flash, PAGE_SIZE);
    assert_eq!(offsets.save_len, 0);
    assert_eq!(offsets.save_len_flash, 0);
    assert_eq!(offsets.data_page, (FLASH_PAGES - 1) as u16);
    assert_eq!(offsets.save_page, None);
    assert_eq!(bytes[0], 0x42);
    assert!(bytes[1..].iter().all(|byte| *byte == 0xff));
}

#[test]
fn one_byte_save_pads_to_a_sector_and_uses_save_base_addresses() {
    let mut layout = Layout::new();
    layout.begin_save().unwrap();
    assert_eq!(layout.field("save", FieldKind::Bytes), Ok(0));
    layout.write(&[0x42]).unwrap();

    let (offsets, bytes) = layout.finalize().unwrap();
    assert_eq!(offsets.data_len, 0);
    assert_eq!(offsets.data_len_flash, 0);
    assert_eq!(offsets.save_len, 1);
    assert_eq!(offsets.save_len_flash, SAVE_SECTOR);
    assert_eq!(offsets.save_page, Some((FLASH_PAGES - 16) as u16));
    assert_eq!(offsets.data_page, (FLASH_PAGES - 16) as u16);
    assert_eq!(bytes[0], 0x42);
    assert!(bytes[1..].iter().all(|byte| *byte == 0xff));
}

#[test]
fn second_begin_save_fails() {
    let mut layout = Layout::new();
    layout.begin_save().unwrap();
    assert_eq!(layout.begin_save(), Err(LayoutError::SaveAlreadyBegun));
}

#[test]
fn writes_reject_addresses_above_24_bits() {
    let mut layout = Layout::new();
    layout.write(&vec![0; 0x01_000000]).unwrap();
    assert_eq!(
        layout.field("too_far", FieldKind::Bytes),
        Err(LayoutError::AddressOverflow(0x01_000000))
    );
}
