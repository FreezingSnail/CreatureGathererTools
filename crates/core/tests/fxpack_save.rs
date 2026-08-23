use creaturevm::fxpack::{
    layout::{FLASH_PAGES, Layout, LayoutError, SAVE_SECTOR},
    pack,
    spec::{LayoutSpec, SaveEntry, SaveSpec},
};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let suffix = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        Self(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join(format!("fxpack-save-{}-{suffix}", std::process::id())),
        )
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn two_sector_spec() -> LayoutSpec {
    LayoutSpec {
        entries: vec![],
        save: Some(SaveSpec {
            sectors: 2,
            entries: vec![
                SaveEntry {
                    name: "save_main".to_owned(),
                    offset: 0,
                },
                SaveEntry {
                    name: "save_log".to_owned(),
                    offset: SAVE_SECTOR,
                },
            ],
        }),
    }
}

#[test]
fn reserved_two_sector_save_is_erased_and_exactly_sized() {
    let mut layout = Layout::new();
    layout.begin_save().unwrap();
    layout.reserve_save_sectors(2).unwrap();

    let (offsets, bytes) = layout.finalize().unwrap();
    assert_eq!(offsets.save_len, 2 * SAVE_SECTOR);
    assert_eq!(offsets.save_len_flash, 2 * SAVE_SECTOR);
    assert_eq!(offsets.save_page, Some((FLASH_PAGES - 32) as u16));
    assert_eq!(bytes, vec![0xff; (2 * SAVE_SECTOR) as usize]);
}

#[test]
fn pack_renders_named_save_offsets() {
    let output = TestDir::new();
    let outcome = pack::pack(
        &two_sector_spec(),
        &[],
        &output.path().join("dist"),
        &output.path().join("include/fxdata.h"),
    )
    .unwrap();

    let header = std::fs::read_to_string(&outcome.header).unwrap();
    assert!(header.contains("constexpr uint16_t FX_SAVE_PAGE  = 0xFFE0;\n"));
    assert!(header.contains("constexpr uint24_t FX_SAVE_BYTES = 8192;\n"));
    assert!(header.contains(
        "// FX save section offsets\nconstexpr uint24_t save_main = 0x000000;\nconstexpr uint24_t save_log = 0x001000;\n"
    ));

    let save = std::fs::read(outcome.artifacts.save_bin.unwrap()).unwrap();
    assert_eq!(save, vec![0xff; (2 * SAVE_SECTOR) as usize]);
}

#[test]
fn unaligned_save_offset_is_rejected() {
    let mut layout = Layout::new();
    layout.begin_save().unwrap();
    layout.reserve_save_sectors(2).unwrap();

    assert_eq!(
        layout.save_offset_field("save_bad", SAVE_SECTOR - 1),
        Err(LayoutError::SaveOffsetUnaligned(SAVE_SECTOR - 1))
    );
}

#[test]
fn out_of_range_save_offset_is_rejected() {
    let mut layout = Layout::new();
    layout.begin_save().unwrap();
    layout.reserve_save_sectors(2).unwrap();

    assert_eq!(
        layout.save_offset_field("save_bad", 2 * SAVE_SECTOR),
        Err(LayoutError::SaveOffsetOutOfRange(2 * SAVE_SECTOR))
    );
}

#[test]
fn reserving_save_sectors_requires_save_section() {
    assert_eq!(
        Layout::new().reserve_save_sectors(2),
        Err(LayoutError::SaveNotBegun)
    );
}

#[test]
fn pack_without_save_emits_no_save_constants_or_binary() {
    let output = TestDir::new();
    let outcome = pack::pack(
        &LayoutSpec {
            entries: vec![],
            save: None,
        },
        &[],
        &output.path().join("dist"),
        &output.path().join("include/fxdata.h"),
    )
    .unwrap();

    let header = std::fs::read_to_string(&outcome.header).unwrap();
    assert!(!header.contains("FX_SAVE_PAGE"));
    assert!(!header.contains("FX_SAVE_BYTES"));
    assert!(outcome.artifacts.save_bin.is_none());
    assert!(!output.path().join("dist/fxdata-save.bin").exists());
}
