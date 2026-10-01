use anyhow::{Context, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub struct Stats {
    pub archived: usize,
    pub converted: usize,
}

struct Prepared {
    source: PathBuf,
    archive: PathBuf,
    original: Vec<u8>,
    converted: Vec<u8>,
}

/// Convert every sprite/font PNG referenced by a sprite source config.
///
/// All source images are decoded and encoded before the archive phase begins.
/// Archive destinations are immutable: existing identical files are reused,
/// while differing files abort the operation before any source is rewritten.
pub fn convert(config_path: &Path, archive_dir: &Path) -> anyhow::Result<Stats> {
    let references = super::sprite_source::referenced_images(config_path)?;
    let mut prepared = Vec::with_capacity(references.len());
    for reference in references {
        let original = fs::read(&reference.path)
            .with_context(|| format!("reading source PNG {}", reference.path.display()))?;
        let converted = convert_png_bytes(&original, &reference.path)?;
        prepared.push(Prepared {
            source: reference.path,
            archive: archive_dir.join(reference.archive_relative),
            original,
            converted,
        });
    }

    let archived = archive_originals(&prepared)?;
    for item in &prepared {
        fs::write(&item.source, &item.converted)
            .with_context(|| format!("rewriting source PNG {}", item.source.display()))?;
    }
    Ok(Stats {
        archived,
        converted: prepared.len(),
    })
}

fn convert_png_bytes(input: &[u8], source: &Path) -> anyhow::Result<Vec<u8>> {
    let (mut pixels, width, height) = super::sprite::decode_rgba_bytes(input, source)?;
    for rgba in pixels.chunks_exact_mut(4) {
        let value = if rgba[0] >= 128 { 255 } else { 0 };
        rgba[0] = value;
        rgba[1] = value;
        rgba[2] = value;
    }
    encode_rgba8(&pixels, width, height, source)
}

fn encode_rgba8(pixels: &[u8], width: u32, height: u32, source: &Path) -> anyhow::Result<Vec<u8>> {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .context("image dimensions are too large")?;
    ensure!(
        pixels.len() == expected_len,
        "decoded RGBA size does not match {}x{} for {}",
        width,
        height,
        source.display()
    );
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .with_context(|| format!("encoding PNG header for {}", source.display()))?;
        writer
            .write_image_data(pixels)
            .with_context(|| format!("encoding PNG pixels for {}", source.display()))?;
    }
    Ok(bytes)
}

fn archive_originals(prepared: &[Prepared]) -> anyhow::Result<usize> {
    let mut destinations = HashMap::<PathBuf, &[u8]>::new();
    for item in prepared {
        reject_source_archive_alias(item)?;
        if let Some(previous) = destinations.insert(item.archive.clone(), &item.original)
            && previous != &item.original
        {
            bail!(
                "archive conflict: multiple source PNGs map to {} with different bytes",
                item.archive.display()
            );
        }
        if item.archive.exists() {
            let archived = fs::read(&item.archive)
                .with_context(|| format!("reading archive {}", item.archive.display()))?;
            ensure_archive_or_idempotent(item, &archived)?;
        }
    }

    let mut created = Vec::new();
    let result = (|| {
        let mut count = 0;
        for item in prepared {
            if item.archive.exists() {
                continue;
            }
            if let Some(parent) = item.archive.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("creating archive directory {}", parent.display()))?;
            }
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&item.archive)
                .with_context(|| format!("creating archive {}", item.archive.display()))?;
            created.push(item.archive.clone());
            file.write_all(&item.original)
                .with_context(|| format!("writing archive {}", item.archive.display()))?;
            file.flush()
                .with_context(|| format!("flushing archive {}", item.archive.display()))?;
            drop(file);
            let archived = fs::read(&item.archive)
                .with_context(|| format!("verifying archive {}", item.archive.display()))?;
            ensure_archive_bytes(&item.archive, &item.original, &archived)?;
            count += 1;
        }
        Ok(count)
    })();
    if result.is_err() {
        for path in created {
            let _ = fs::remove_file(path);
        }
    }
    result
}

fn reject_source_archive_alias(item: &Prepared) -> anyhow::Result<()> {
    if item.source == item.archive {
        bail!(
            "archive path {} is the source PNG; choose a separate archive directory",
            item.archive.display()
        );
    }
    if item.archive.exists() {
        let source = fs::canonicalize(&item.source)
            .with_context(|| format!("resolving source {}", item.source.display()))?;
        let archive = fs::canonicalize(&item.archive)
            .with_context(|| format!("resolving archive {}", item.archive.display()))?;
        ensure!(
            source != archive,
            "archive path {} is the source PNG; choose a separate archive directory",
            item.archive.display()
        );
    }
    Ok(())
}

fn ensure_archive_or_idempotent(item: &Prepared, archived: &[u8]) -> anyhow::Result<()> {
    if archived == item.original {
        return Ok(());
    }
    if let Ok(archive_converted) = convert_png_bytes(archived, &item.archive)
        && item.original == archive_converted
    {
        return Ok(());
    }
    ensure_archive_bytes(&item.archive, &item.original, archived)
}

fn ensure_archive_bytes(path: &Path, expected: &[u8], actual: &[u8]) -> anyhow::Result<()> {
    let expected_hash = sha256(expected);
    let actual_hash = sha256(actual);
    ensure!(
        expected_hash == actual_hash && expected == actual,
        "archive conflict at {}: existing bytes differ (sha256 expected {}, found {})",
        path.display(),
        hex_hash(&expected_hash),
        hex_hash(&actual_hash)
    );
    Ok(())
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn hex_hash(hash: &[u8; 32]) -> String {
    hash.iter().map(|byte| format!("{byte:02x}")).collect()
}
