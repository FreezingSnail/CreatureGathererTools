use creaturevm::fxpack::{
    bw_placeholders,
    sprite::{self, Params},
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/testdata/bw-placeholders"
);
const NAMES: [&str; 5] = [
    "unmasked.png",
    "masked.png",
    "partial-alpha.png",
    "palette.png",
    "gray.png",
];
static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("cgfx-bw-placeholders-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(FIXTURES).join(name)
}

fn setup() -> (TestDir, PathBuf, PathBuf) {
    let dir = TestDir::new();
    for name in NAMES {
        fs::copy(fixture(name), dir.path().join(name)).unwrap();
    }
    let config = dir.path().join("fxsprites.toml");
    fs::write(
        &config,
        r#"[[set]]
output = "unused.h"
images = ["unmasked.png", "masked.png", "partial-alpha.png", "palette.png", "unmasked.png"]

[strings]
output = "unused-strings.h"
font = "gray.png"
input = "unused-strings.txt"
glyph_width = 1
glyph_height = 1
order = []
"#,
    )
    .unwrap();
    let archive = dir.path().join("archive");
    (dir, config, archive)
}

fn params(shades: u8) -> Params {
    Params {
        width: 0,
        height: 0,
        shades,
        spacing: 0,
    }
}

fn expanded(path: &Path) -> (u32, u32, Vec<u8>) {
    let file = fs::File::open(path).unwrap();
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes).unwrap();
    bytes.truncate(info.buffer_size());
    let rgba = match info.color_type {
        png::ColorType::Rgba => bytes,
        png::ColorType::Rgb => bytes
            .chunks_exact(3)
            .flat_map(|px| [px[0], px[1], px[2], 255])
            .collect(),
        png::ColorType::Grayscale => bytes
            .iter()
            .flat_map(|&gray| [gray, gray, gray, 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => bytes
            .chunks_exact(2)
            .flat_map(|px| [px[0], px[0], px[0], px[1]])
            .collect(),
        png::ColorType::Indexed => panic!("EXPAND left indexed PNG"),
    };
    (info.width, info.height, rgba)
}

fn assert_rgba8(path: &Path) {
    let file = fs::File::open(path).unwrap();
    let decoder = png::Decoder::new(file);
    let mut reader = decoder.read_info().unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut bytes = vec![0; reader.output_buffer_size()];
    reader.next_frame(&mut bytes).unwrap();
}

#[test]
fn converts_all_fixture_kinds_and_preserves_archive_restore_and_alpha() {
    let (dir, config, archive) = setup();
    let originals: Vec<Vec<u8>> = NAMES
        .iter()
        .map(|name| fs::read(dir.path().join(name)).unwrap())
        .collect();
    let result = bw_placeholders::convert(&config, &archive).unwrap();
    assert_eq!(result.archived, 5);
    assert_eq!(result.converted, 5);

    for (index, name) in NAMES.iter().enumerate() {
        let source = dir.path().join(name);
        let archived = archive.join(name);
        assert_eq!(fs::read(&archived).unwrap(), originals[index]);
        assert_eq!(
            fs::read(&archived).unwrap(),
            fs::read(fixture(name)).unwrap()
        );
        assert_eq!(
            sprite::encode_png_legacy(&source, params(2)).unwrap().bytes,
            sprite::encode_png_legacy(&archived, params(2))
                .unwrap()
                .bytes,
            "converted {} does not match 1bpp encoding",
            name
        );
        assert_eq!(
            sprite::encode_png_legacy(&archived, params(4))
                .unwrap()
                .bytes,
            sprite::encode_png_legacy(&fixture(name), params(4))
                .unwrap()
                .bytes,
            "archive restore encoding differs for {}",
            name
        );
        let (source_w, source_h, source_rgba) = expanded(&archived);
        let (converted_w, converted_h, converted_rgba) = expanded(&source);
        assert_eq!((converted_w, converted_h), (source_w, source_h));
        assert_eq!(
            converted_rgba
                .chunks_exact(4)
                .map(|pixel| pixel[3])
                .collect::<Vec<_>>(),
            source_rgba
                .chunks_exact(4)
                .map(|pixel| pixel[3])
                .collect::<Vec<_>>(),
            "alpha changed for {}",
            name
        );
        assert_rgba8(&source);
    }
}

#[test]
fn second_run_is_idempotent_and_never_overwrites_archive() {
    let (_dir, config, archive) = setup();
    let first = bw_placeholders::convert(&config, &archive).unwrap();
    let archived_before: Vec<Vec<u8>> = NAMES
        .iter()
        .map(|name| fs::read(archive.join(name)).unwrap())
        .collect();
    let converted_before: Vec<Vec<u8>> = NAMES
        .iter()
        .map(|name| fs::read(config.parent().unwrap().join(name)).unwrap())
        .collect();
    let second = bw_placeholders::convert(&config, &archive).unwrap();
    assert_eq!(first.archived, 5);
    assert_eq!(second.archived, 0);
    assert_eq!(second.converted, 5);
    for (index, name) in NAMES.iter().enumerate() {
        assert_eq!(
            fs::read(archive.join(name)).unwrap(),
            archived_before[index]
        );
        assert_eq!(
            fs::read(config.parent().unwrap().join(name)).unwrap(),
            converted_before[index]
        );
    }
}

#[test]
fn archive_conflict_aborts_before_any_png_rewrite_or_new_archive_copy() {
    let (dir, config, archive) = setup();
    let originals: Vec<Vec<u8>> = NAMES
        .iter()
        .map(|name| fs::read(dir.path().join(name)).unwrap())
        .collect();
    fs::create_dir_all(&archive).unwrap();
    fs::write(archive.join("masked.png"), b"not-the-original").unwrap();

    let error = bw_placeholders::convert(&config, &archive).unwrap_err();
    assert!(error.to_string().contains("archive conflict"));
    for (index, name) in NAMES.iter().enumerate() {
        assert_eq!(fs::read(dir.path().join(name)).unwrap(), originals[index]);
    }
    assert!(!archive.join("unmasked.png").exists());
    assert_eq!(
        fs::read(archive.join("masked.png")).unwrap(),
        b"not-the-original"
    );
}

#[test]
fn archive_write_failure_rolls_back_prior_archive_copies() {
    let (dir, config, archive) = setup();
    let nested_source = dir.path().join("blocked/masked.png");
    fs::create_dir_all(nested_source.parent().unwrap()).unwrap();
    fs::copy(fixture("masked.png"), &nested_source).unwrap();
    fs::write(
        &config,
        r#"[[set]]
output = "unused.h"
images = ["unmasked.png", "blocked/masked.png"]
"#,
    )
    .unwrap();
    let unmasked_before = fs::read(dir.path().join("unmasked.png")).unwrap();
    fs::create_dir_all(&archive).unwrap();
    fs::write(archive.join("blocked"), b"archive parent is a file").unwrap();

    assert!(bw_placeholders::convert(&config, &archive).is_err());
    assert_eq!(
        fs::read(dir.path().join("unmasked.png")).unwrap(),
        unmasked_before
    );
    assert!(!archive.join("unmasked.png").exists());
    assert_eq!(
        fs::read(archive.join("blocked")).unwrap(),
        b"archive parent is a file"
    );
}

#[test]
fn cli_prints_exact_success_message() {
    let (_dir, config, archive) = setup();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_cgfx-tools"))
        .args([
            "--bw-placeholders",
            config.to_str().unwrap(),
            "--archive-dir",
            archive.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "bw-placeholders: archived 5, converted 5.\n"
    );
}
