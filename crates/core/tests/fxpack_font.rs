use creaturevm::fxpack::font::{Atlas, parse_strings, render_strings};
use std::fs::{self, File};
use std::path::PathBuf;
use std::sync::OnceLock;

fn fixture_path(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("fxpack_font_tests");
    fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

fn atlas() -> Atlas {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    let path = PATH.get_or_init(|| {
        const GLYPH_W: u32 = 1;
        const GLYPH_H: u32 = 6;
        const GLYPHS: u32 = 98;
        let path = fixture_path("indexed_font.png");
        let mut data = vec![0; (GLYPH_W * GLYPH_H * GLYPHS) as usize];
        for pixel in data.iter_mut().skip(97).step_by(GLYPHS as usize) {
            *pixel = 1;
        }

        let file = File::create(&path).unwrap();
        let mut encoder = png::Encoder::new(file, GLYPH_W * GLYPHS, GLYPH_H);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_palette(vec![0, 0, 0, 255, 255, 255]);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&data).unwrap();
        drop(writer);
        path
    });

    Atlas::load(path, 1, 6, 0).unwrap()
}

#[test]
fn parses_namespaces_in_source_order() {
    let strings = parse_strings(
        "namespace CreatureNames {\n    creature0 = \"SquibbleSnail\"\n}\n\nnamespace MoveNames {\n    amove0 = \"smite\"\n}\n",
    )
    .unwrap();

    assert_eq!(
        strings,
        vec![
            ("creature0".to_owned(), "SquibbleSnail".to_owned()),
            ("amove0".to_owned(), "smite".to_owned()),
        ]
    );
}

#[test]
fn rejects_duplicate_identifiers() {
    let error = parse_strings(
        "namespace First {\nname = \"one\"\n}\nnamespace Second {\nname = \"two\"\n}",
    )
    .unwrap_err();

    assert!(error.to_string().contains("duplicate identifier: name"));
}

#[test]
fn renders_joined_greyscale_images_from_indexed_atlas() {
    let images = render_strings(
        &atlas(),
        &[
            ("first".to_owned(), "a".to_owned()),
            ("second".to_owned(), "aa".to_owned()),
        ],
        4,
    )
    .unwrap();

    assert_eq!(
        images
            .iter()
            .map(|image| image.name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second"]
    );
    assert_eq!(
        (
            images[0].sheet.frames,
            images[0].sheet.width,
            images[0].sheet.height
        ),
        (1, 2, 8)
    );
    assert_eq!(images[0].sheet.bytes, vec![63, 192, 63, 192, 63, 192]);
    assert_eq!(
        (
            images[1].sheet.frames,
            images[1].sheet.width,
            images[1].sheet.height
        ),
        (1, 4, 8)
    );
    assert_eq!(
        images[1].sheet.bytes,
        vec![63, 63, 192, 192, 63, 63, 192, 192, 63, 63, 192, 192]
    );
}

#[test]
fn rejects_unmapped_characters_with_string_name() {
    let error = match render_strings(&atlas(), &[("bad_name".to_owned(), "z".to_owned())], 4) {
        Ok(_) => panic!("unmapped character must fail"),
        Err(error) => error,
    };
    let message = error.to_string();

    assert!(message.contains("unmapped character 'z'"));
    assert!(message.contains("string 'bad_name'"));
}

#[test]
fn rendering_is_deterministic() {
    let strings = vec![("first".to_owned(), "aa".to_owned())];
    let first = render_strings(&atlas(), &strings, 4).unwrap();
    let second = render_strings(&atlas(), &strings, 4).unwrap();

    assert_eq!(first[0].name, second[0].name);
    assert_eq!(first[0].sheet.bytes, second[0].sheet.bytes);
    assert_eq!(first[0].sheet.frames, second[0].sheet.frames);
    assert_eq!(first[0].sheet.width, second[0].sheet.width);
    assert_eq!(first[0].sheet.height, second[0].sheet.height);
}
