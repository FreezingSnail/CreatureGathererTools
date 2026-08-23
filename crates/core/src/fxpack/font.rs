use anyhow::{Context, bail};
use std::path::Path;

use super::sprite::{self, Params, Sheet};

/// A one-row bitmap font atlas whose glyph positions match their character code.
pub struct Atlas {
    pub glyph_w: u16,
    pub glyph_h: u16,
    pixels: Vec<u8>,
    img_w: u32,
    cols: u16,
    first_char: u8,
}

pub struct StringImage {
    pub name: String,
    pub sheet: Sheet,
}

impl Atlas {
    pub fn load(png: &Path, glyph_w: u16, glyph_h: u16, first_char: u8) -> anyhow::Result<Self> {
        if glyph_w == 0 || glyph_h == 0 {
            bail!("font glyph dimensions must be non-zero");
        }

        let (pixels, img_w, img_h) = sprite::decode_rgba(png)?;
        let glyph_w_u32 = u32::from(glyph_w);
        if img_w % glyph_w_u32 != 0 {
            bail!(
                "font atlas {} width {} is not divisible by glyph width {}",
                png.display(),
                img_w,
                glyph_w
            );
        }
        if img_h != u32::from(glyph_h) {
            bail!(
                "font atlas {} height {} does not match glyph height {}",
                png.display(),
                img_h,
                glyph_h
            );
        }

        let cols = u16::try_from(img_w / glyph_w_u32)
            .context("font atlas has more glyphs than supported")?;
        if cols == 0 {
            bail!("font atlas {} contains no glyphs", png.display());
        }

        Ok(Self {
            glyph_w,
            glyph_h,
            pixels,
            img_w,
            cols,
            first_char,
        })
    }

    fn blit_glyph(&self, index: u16, dest: &mut [u8], dest_w: u32, dest_x: u32) {
        let src_x = u32::from(index) * u32::from(self.glyph_w);
        for y in 0..u32::from(self.glyph_h) {
            for x in 0..u32::from(self.glyph_w) {
                let src = ((y * self.img_w + src_x + x) * 4) as usize;
                let dst = ((y * dest_w + dest_x + x) * 4) as usize;
                dest[dst..dst + 4].copy_from_slice(&self.pixels[src..src + 4]);
            }
        }
    }

    fn char_index(&self, character: char) -> Option<u16> {
        // text2bmp.py indexes its atlas with the raw character code. The first
        // usable glyph may be space, but preceding atlas cells preserve that
        // codepoint addressing.
        let index = u16::try_from(character as u32).ok()?;
        (index < self.cols).then_some(index)
    }

    #[allow(dead_code)]
    fn first_char(&self) -> u8 {
        self.first_char
    }
}

/// Parse strings.txt namespaces into `(identifier, text)` pairs in source order.
pub fn load_strings(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    parse_strings(&content)
}

pub fn parse_strings(content: &str) -> anyhow::Result<Vec<(String, String)>> {
    let mut result = Vec::new();
    let mut rest = content;

    while let Some(namespace_start) = rest.find("namespace ") {
        rest = &rest[namespace_start + "namespace ".len()..];
        let Some(open_brace) = rest.find('{') else {
            break;
        };
        rest = &rest[open_brace + 1..];
        let Some(close_brace) = rest.find('}') else {
            break;
        };
        let block = &rest[..close_brace];
        rest = &rest[close_brace + 1..];

        for line in block.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .unwrap_or(value);

            if result.iter().any(|(existing, _)| existing == key) {
                bail!("duplicate identifier: {key}");
            }
            result.push((key.to_owned(), value.to_owned()));
        }
    }

    Ok(result)
}

/// Render strings as the `[original | inverted]` images used by text2bmp.py.
pub fn render_strings(
    atlas: &Atlas,
    strings: &[(String, String)],
    shades: u8,
) -> anyhow::Result<Vec<StringImage>> {
    strings
        .iter()
        .map(|(name, text)| {
            Ok(StringImage {
                name: name.clone(),
                sheet: render_one(atlas, name, text, shades)?,
            })
        })
        .collect()
}

fn render_one(atlas: &Atlas, name: &str, text: &str, shades: u8) -> anyhow::Result<Sheet> {
    if text.is_empty() {
        return Ok(Sheet {
            bytes: Vec::new(),
            frames: 1,
            width: 0,
            height: 0,
        });
    }

    let glyph_w = u32::from(atlas.glyph_w);
    let glyph_h = u32::from(atlas.glyph_h);
    let padded_h = glyph_h.div_ceil(8) * 8;
    let text_w = u32::try_from(text.chars().count())
        .context("string is too long to render")?
        .checked_mul(glyph_w)
        .context("string image width overflows")?;
    let image_w = text_w
        .checked_mul(2)
        .context("joined string image width overflows")?;
    let pixel_count = image_w
        .checked_mul(padded_h)
        .and_then(|count| count.checked_mul(4))
        .context("string image dimensions are too large")?;
    let mut pixels = vec![0; usize::try_from(pixel_count).context("string image is too large")?];
    for rgba in pixels.chunks_exact_mut(4) {
        rgba[3] = 255;
    }

    for (position, character) in text.chars().enumerate() {
        let index = atlas.char_index(character).ok_or_else(|| {
            anyhow::anyhow!("unmapped character '{}' in string '{}'", character, name)
        })?;
        let dest_x = u32::try_from(position)
            .context("string is too long to render")?
            .checked_mul(glyph_w)
            .context("string image width overflows")?;
        atlas.blit_glyph(index, &mut pixels, image_w, dest_x);
    }

    for y in 0..padded_h {
        for x in 0..text_w {
            let src = ((y * image_w + x) * 4) as usize;
            let dst = ((y * image_w + text_w + x) * 4) as usize;
            pixels[dst] = 255 - pixels[src];
            pixels[dst + 1] = 255 - pixels[src + 1];
            pixels[dst + 2] = 255 - pixels[src + 2];
            pixels[dst + 3] = 255;
        }
    }

    sprite::encode_rgba(
        &pixels,
        image_w,
        padded_h,
        Params {
            width: u16::try_from(text_w)
                .context("string image width exceeds sprite width limit")?,
            height: u16::try_from(padded_h)
                .context("string image height exceeds sprite height limit")?,
            shades,
            spacing: 0,
        },
    )
    .with_context(|| format!("encoding string image for '{name}'"))
}
