use anyhow::{Context, bail};
use std::path::{Path, PathBuf};

use super::{
    carray, header,
    image::{self, Artifacts},
    layout::{Field, FieldKind, Layout},
    legacy::{self, LegacyBridgeInputs, LegacyOutput},
    spec::{Entry, LayoutSpec, Source},
    sprite::{self, Params},
};

pub struct BuilderBytes<'a> {
    pub key: &'a str,
    pub bytes: Vec<u8>,
    pub namespace: Option<&'a str>,
}

pub struct PackOutcome {
    pub header: PathBuf,
    pub artifacts: Artifacts,
    pub fields: Vec<Field>,
}

/// Pack an FX image without legacy symbolic-reference bridge support.
///
/// Existing raw, C-array, image, and builder packing behavior is unchanged.
/// Layouts with `source = { legacy = "..." }` must call
/// [`pack_with_legacy`] instead.
pub fn pack(
    spec: &LayoutSpec,
    builders: &[BuilderBytes<'_>],
    out_dir: &Path,
    header_path: &Path,
) -> anyhow::Result<PackOutcome> {
    pack_with_legacy(spec, builders, out_dir, header_path, None)
}

/// Pack an FX image, optionally resolving `legacy` layout entries through the
/// isolated `fxdata-build.py` bridge.
pub fn pack_with_legacy(
    spec: &LayoutSpec,
    builders: &[BuilderBytes<'_>],
    out_dir: &Path,
    header_path: &Path,
    legacy_inputs: Option<&LegacyBridgeInputs<'_>>,
) -> anyhow::Result<PackOutcome> {
    validate_builder_keys(spec, builders)?;
    let symbols: Vec<String> = spec
        .legacy_symbols()
        .into_iter()
        .map(str::to_owned)
        .collect();
    let legacy_output = if symbols.is_empty() {
        None
    } else {
        let inputs = legacy_inputs.context(
            "layout contains legacy sources, but no legacy bridge inputs were supplied; use cgfx-tools --project <project> --pack",
        )?;
        Some(
            legacy::extract(inputs, &symbols)
                .context("extracting symbolic legacy declarations through fxdata-build.py")?,
        )
    };

    let mut layout = Layout::new();
    let mut current_namespace: Option<&str> = None;
    for entry in &spec.entries {
        let entry_namespace = entry.namespace.as_deref();
        if current_namespace != entry_namespace {
            if current_namespace.is_some() {
                layout.end_namespace()?;
            }
            if let Some(namespace) = entry_namespace {
                layout.begin_namespace(namespace);
            }
            current_namespace = entry_namespace;
        }
        if let Some(align) = entry.align {
            layout.pad(align, false);
        }
        let (bytes, kind) = resolve_entry(entry, builders, legacy_output.as_ref())
            .with_context(|| format!("resolving entry {:?}", entry.name))?;
        layout.field(&entry.name, kind)?;
        layout.write(&bytes)?;
    }
    if current_namespace.is_some() {
        layout.end_namespace()?;
    }

    if let Some(save) = &spec.save {
        layout.begin_save()?;
        layout.reserve_save_sectors(save.sectors)?;
        for entry in &save.entries {
            layout.save_offset_field(&entry.name, entry.offset)?;
        }
    }

    let fields = layout.fields().to_vec();
    let (offsets, bytes) = layout.finalize()?;
    let header_text = header::render(&fields, &offsets);
    let header_parent = header_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(header_parent)
        .with_context(|| format!("creating directory for {}", header_path.display()))?;
    std::fs::write(header_path, header_text)
        .with_context(|| format!("writing {}", header_path.display()))?;

    let artifacts = image::write(&bytes, &offsets, out_dir, "fxdata")?;
    Ok(PackOutcome {
        header: header_path.to_owned(),
        artifacts,
        fields,
    })
}

fn validate_builder_keys(spec: &LayoutSpec, builders: &[BuilderBytes<'_>]) -> anyhow::Result<()> {
    let spec_keys = spec.builder_keys();
    let available: Vec<&str> = builders.iter().map(|builder| builder.key).collect();
    for key in &spec_keys {
        if !available.contains(key) {
            bail!("unknown builder key {key:?} in layout spec; available: {available:?}");
        }
    }
    for builder in builders {
        if !spec_keys.contains(&builder.key) {
            bail!(
                "unused builder key {:?}; spec uses: {spec_keys:?}",
                builder.key
            );
        }
    }
    Ok(())
}

fn resolve_entry(
    entry: &Entry,
    builders: &[BuilderBytes<'_>],
    legacy_output: Option<&LegacyOutput>,
) -> anyhow::Result<(Vec<u8>, FieldKind)> {
    match &entry.source {
        Source::Raw(path) => {
            let bytes = std::fs::read(path)
                .with_context(|| format!("reading raw source {}", path.display()))?;
            Ok((bytes, FieldKind::Bytes))
        }
        Source::CArray(spec) => {
            let bytes = if let Some(symbol) = spec.symbol() {
                carray::extract_symbol_bytes(spec.path(), symbol)?
            } else {
                carray::extract_bytes(spec.path())?
            };
            Ok((bytes, FieldKind::Bytes))
        }
        Source::Image {
            path,
            width,
            height,
            shades,
            spacing,
        } => {
            let sheet = sprite::encode_png(
                path,
                Params {
                    width: *width,
                    height: *height,
                    shades: *shades,
                    spacing: *spacing,
                },
            )?;
            // fxdata-build.py's image_t prefixes each sheet with u16 width and height.
            let mut bytes = Vec::with_capacity(4 + sheet.bytes.len());
            bytes.extend_from_slice(&sheet.width.to_le_bytes());
            bytes.extend_from_slice(&sheet.height.to_le_bytes());
            bytes.extend_from_slice(&sheet.bytes);
            Ok((
                bytes,
                FieldKind::Image {
                    frames: sheet.frames,
                    width: sheet.width,
                    height: sheet.height,
                },
            ))
        }
        Source::Builder(key) => {
            let builder = builders
                .iter()
                .find(|builder| builder.key == key)
                .expect("builder keys validated before entry resolution");
            Ok((builder.bytes.clone(), FieldKind::Bytes))
        }
        Source::Legacy(symbol) => {
            let output = legacy_output
                .context("layout contains a legacy source, but the legacy bridge did not run")?;
            Ok((output.bytes(symbol)?.to_vec(), FieldKind::Bytes))
        }
    }
}
