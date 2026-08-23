use creaturevm::fxpack::sprite::{Params, encode_png, encode_rgba};
use std::fs::{self, File};
use std::path::PathBuf;

fn params(width: u16, height: u16, shades: u8) -> Params {
    Params {
        width,
        height,
        shades,
        spacing: 0,
    }
}

fn fixture_path(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("fxpack_sprite_tests");
    fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

fn write_png(path: &std::path::Path, width: u32, height: u32, color: png::ColorType, data: &[u8]) {
    let file = File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(color);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(data).unwrap();
}

#[test]
fn encodes_grayscale_tile_in_python_plane_order() {
    let path = fixture_path("grayscale_4x8.png");
    let mut pixels = Vec::new();
    for y in 0..8 {
        pixels.extend_from_slice(&[
            [0, 64, 128, 192, 0, 64, 128, 192][y],
            0,
            255,
            [191, 192, 191, 192, 191, 192, 191, 192][y],
        ]);
    }
    write_png(&path, 4, 8, png::ColorType::Grayscale, &pixels);

    let sheet = encode_png(&path, params(4, 8, 4)).unwrap();

    assert_eq!(sheet.frames, 1);
    assert_eq!(sheet.width, 4);
    assert_eq!(sheet.height, 8);
    assert_eq!(
        sheet.bytes,
        vec![238, 0, 255, 255, 204, 0, 255, 255, 136, 0, 255, 170]
    );
}

#[test]
fn encodes_two_vertical_tiles() {
    let path = fixture_path("two_tiles_8x16.png");
    let mut pixels = vec![0; 8 * 8];
    pixels.extend(std::iter::repeat_n(255, 8 * 8));
    write_png(&path, 8, 16, png::ColorType::Grayscale, &pixels);

    let sheet = encode_png(&path, params(8, 8, 2)).unwrap();

    assert_eq!((sheet.frames, sheet.width, sheet.height), (2, 8, 8));
    assert_eq!(sheet.bytes, [vec![0; 8], vec![255; 8]].concat());
}

#[test]
fn rejects_invalid_shade_count() {
    let error = match encode_rgba(&[0; 4], 1, 1, params(1, 1, 5)) {
        Ok(_) => panic!("invalid shade count must fail"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("got 5"));
}

#[test]
fn rejects_non_integral_tile_dimensions() {
    let error = match encode_rgba(&[0; 4 * 8 * 4], 4, 8, params(3, 8, 2)) {
        Ok(_) => panic!("non-integral tiles must fail"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("invalid sprite dimensions"));
}

#[test]
fn encode_rgba_is_deterministic() {
    let pixels = [
        0, 0, 0, 255, 255, 255, 255, 255, 128, 128, 128, 255, 192, 192, 192, 255,
    ];
    let first = encode_rgba(&pixels, 2, 2, params(2, 2, 4)).unwrap();
    let second = encode_rgba(&pixels, 2, 2, params(2, 2, 4)).unwrap();

    assert_eq!(first.bytes, second.bytes);
    assert_eq!(
        (first.frames, first.width, first.height),
        (second.frames, second.width, second.height)
    );
}

#[test]
fn interleaves_masks_when_any_pixel_is_transparent() {
    let mut pixels = Vec::new();
    for alpha in [255, 0, 127, 128, 255, 0, 255, 255] {
        pixels.extend_from_slice(&[255, 255, 255, alpha]);
    }

    let sheet = encode_rgba(&pixels, 1, 8, params(1, 8, 2)).unwrap();

    assert_eq!(sheet.bytes, vec![255, 217]);
}
