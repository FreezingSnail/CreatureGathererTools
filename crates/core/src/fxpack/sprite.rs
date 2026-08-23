use anyhow::{Context, bail};
use std::path::Path;

pub struct Sheet {
    pub bytes: Vec<u8>,
    pub frames: u16,
    pub width: u16,
    pub height: u16,
}

pub struct Params {
    pub width: u16,
    pub height: u16,
    pub shades: u8,
    pub spacing: u8,
}

pub fn encode_png(path: &Path, params: Params) -> anyhow::Result<Sheet> {
    let (pixels, img_w, img_h) = decode_rgba(path)?;
    encode_rgba(&pixels, img_w, img_h, params)
        .with_context(|| format!("Encoding sprite from {}", path.display()))
}

pub fn encode_rgba(pixels: &[u8], img_w: u32, img_h: u32, params: Params) -> anyhow::Result<Sheet> {
    let shades = params.shades;
    if !(2..=4).contains(&shades) {
        bail!("shades must be 2, 3 or 4, got {shades}");
    }

    let image_len = (img_w as usize)
        .checked_mul(img_h as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .context("image dimensions are too large")?;
    if pixels.len() != image_len {
        bail!(
            "RGBA buffer length {} does not match image dimensions {}x{}",
            pixels.len(),
            img_w,
            img_h
        );
    }

    let sw = if params.width == 0 {
        u16::try_from(img_w).context("image width exceeds sprite width limit")?
    } else {
        params.width
    };
    let sh = if params.height == 0 {
        u16::try_from(img_h).context("image height exceeds sprite height limit")?
    } else {
        params.height
    };
    let spacing = params.spacing as u32;

    let cell_w = sw as u32 + spacing;
    let cell_h = sh as u32 + spacing;
    if cell_w == 0
        || cell_h == 0
        || (img_w + spacing) % cell_w != 0
        || (img_h + spacing) % cell_h != 0
    {
        bail!(
            "invalid sprite dimensions: image {}x{}, tile {}x{} with spacing {}",
            img_w,
            img_h,
            sw,
            sh,
            spacing
        );
    }

    let nw = (img_w + spacing) / cell_w;
    let nh = (img_h + spacing) / cell_h;
    let num_u32 = nw.checked_mul(nh).context("sprite frame count overflows")?;
    let num = u16::try_from(num_u32).context("sprite frame count exceeds u16")?;
    if num == 0 {
        bail!(
            "invalid sprite dimensions: image {}x{}, tile {}x{} with spacing {}",
            img_w,
            img_h,
            sw,
            sh,
            spacing
        );
    }

    let masked = pixels.chunks_exact(4).any(|px| px[3] < 255);
    let sp = (sh as u32).div_ceil(8);
    let bytes_per_column = if masked { 2 } else { 1 };
    let capacity = (num as usize)
        .saturating_mul((shades - 1) as usize)
        .saturating_mul(sp as usize)
        .saturating_mul(sw as usize)
        .saturating_mul(bytes_per_column);
    let mut out = Vec::with_capacity(capacity);

    for n in 0..num as u32 {
        let bx = (n % nw) * cell_w;
        let by = (n / nw) * cell_h;
        for shade in 0..(shades - 1) {
            for p in 0..sp {
                for ix in 0..sw as u32 {
                    let x = bx + ix;
                    let mut byte = 0u8;
                    let mut mask = 0u8;
                    for iy in 0..8u32 {
                        let y = p * 8 + iy;
                        if y >= sh as u32 {
                            break;
                        }
                        let py = by + y;
                        let i = (py * img_w + x) as usize;
                        let rgba = &pixels[i * 4..(i + 1) * 4];
                        byte |= shade_of(rgba, shades, shade) << iy;
                        mask |= mask_of(rgba) << iy;
                    }
                    out.push(byte);
                    if masked {
                        out.push(mask);
                    }
                }
            }
        }
    }

    Ok(Sheet {
        bytes: out,
        frames: num,
        width: sw,
        height: sh,
    })
}

/// Port of Python get_shade: w = (254 + shades) // shades, b = (shade + 1) * w.
fn shade_of(rgba: &[u8], shades: u8, shade: u8) -> u8 {
    let w = (254 + shades as u16) / shades as u16;
    let b = (shade as u16 + 1) * w;
    u8::from(rgba[0] as u16 >= b)
}

fn mask_of(rgba: &[u8]) -> u8 {
    u8::from(rgba[3] >= 128)
}

pub(crate) fn decode_rgba(path: &Path) -> anyhow::Result<(Vec<u8>, u32, u32)> {
    let file = std::fs::File::open(path).with_context(|| format!("Opening {}", path.display()))?;
    let mut decoder = png::Decoder::new(file);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .with_context(|| format!("Reading PNG header from {}", path.display()))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .with_context(|| format!("Decoding PNG frame from {}", path.display()))?;
    buf.truncate(info.buffer_size());

    let (w, h) = (info.width, info.height);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for chunk in buf.chunks_exact(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for &gray in &buf {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = Vec::with_capacity((w * h * 4) as usize);
            for chunk in buf.chunks_exact(2) {
                let gray = chunk[0];
                rgba.extend_from_slice(&[gray, gray, gray, chunk[1]]);
            }
            rgba
        }
        png::ColorType::Indexed => bail!("Indexed PNG not supported for {}", path.display()),
    };

    Ok((rgba, w, h))
}
