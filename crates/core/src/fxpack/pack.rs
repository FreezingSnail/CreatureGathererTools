use anyhow::{Context, bail};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use super::{
    carray, header,
    image::{self, Artifacts},
    layout::{Field, FieldKind, FieldSource, Layout},
    spec::{Entry, LayoutSpec, Source},
    sprite::{self, Params},
    symbol,
};

pub struct BuilderBytes<'a> {
    pub key: &'a str,
    pub bytes: Vec<u8>,
    pub namespace: Option<&'a str>,
}

pub struct PackPlan {
    pub fields: Vec<Field>,
    pub offsets: super::layout::Offsets,
    pub bytes: Vec<u8>,
}

pub struct PackOutcome {
    pub header: PathBuf,
    pub artifacts: Artifacts,
    pub fields: Vec<Field>,
}

/// Resolve all FX entries and compute their finalized layout without writing artifacts.
pub fn plan(spec: &LayoutSpec, builders: &[BuilderBytes<'_>]) -> anyhow::Result<PackPlan> {
    validate_builder_keys(spec, builders)?;
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
        let offsets = field_offsets(layout.fields());
        let (bytes, kind) = resolve_entry(entry, builders, &offsets)
            .with_context(|| format!("resolving entry {:?}", entry.name))?;
        layout.field_with_source(&entry.name, kind, source_metadata(&entry.source))?;
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

    let (fields, offsets, bytes) = layout.finalize_with_fields()?;
    Ok(PackPlan {
        fields,
        offsets,
        bytes,
    })
}

/// Pack an FX image from raw, C-array, typed-symbol, image, and builder sources.
pub fn pack(
    spec: &LayoutSpec,
    builders: &[BuilderBytes<'_>],
    out_dir: &Path,
    header_path: &Path,
) -> anyhow::Result<PackOutcome> {
    let plan = plan(spec, builders)?;
    let header_text = header::render(&plan.fields, &plan.offsets);
    let header_parent = header_path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(header_parent)
        .with_context(|| format!("creating directory for {}", header_path.display()))?;
    std::fs::write(header_path, header_text)
        .with_context(|| format!("writing {}", header_path.display()))?;

    let artifacts = image::write(&plan.bytes, &plan.offsets, out_dir, "fxdata")?;
    Ok(PackOutcome {
        header: header_path.to_owned(),
        artifacts,
        fields: plan.fields,
    })
}

fn field_offsets(fields: &[Field]) -> BTreeMap<String, u32> {
    let mut offsets = BTreeMap::new();
    for field in fields.iter().filter(|field| !field.in_save) {
        let qualified = field.namespace.as_ref().map_or_else(
            || field.name.clone(),
            |namespace| format!("{namespace}::{}", field.name),
        );
        offsets.entry(qualified).or_insert(field.addr);
        // fxdata-build.py resolves an unqualified symbolic initializer to the
        // first declaration with that name, even if its header name is scoped.
        offsets.entry(field.name.clone()).or_insert(field.addr);
    }
    offsets
}

fn source_metadata(source: &Source) -> FieldSource {
    match source {
        Source::Raw(path) => FieldSource::Raw { path: path.clone() },
        Source::CArray(spec) => FieldSource::CArray {
            path: spec.path().to_owned(),
        },
        Source::Symbol(spec) => FieldSource::Symbol {
            path: spec.path.clone(),
        },
        Source::Image { path, .. } => FieldSource::Image { path: path.clone() },
        Source::Builder(key) => FieldSource::Builder { key: key.clone() },
    }
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
    offsets: &BTreeMap<String, u32>,
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
        Source::Symbol(spec) => Ok((
            symbol::extract_symbol_bytes(
                &spec.path,
                &spec.symbol,
                entry.namespace.as_deref(),
                offsets,
            )?,
            FieldKind::Bytes,
        )),
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
            // fxdata-build.py's image_t prefixes each sheet with big-endian u16 width and height.
            let mut bytes = Vec::with_capacity(4 + sheet.bytes.len());
            bytes.extend_from_slice(&sheet.width.to_be_bytes());
            bytes.extend_from_slice(&sheet.height.to_be_bytes());
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
    }
}

#[cfg(test)]
mod tests {
    use super::field_offsets;
    use crate::fxpack::layout::{Field, FieldKind, FieldSource};

    #[test]
    fn unqualified_offsets_keep_the_first_declaration_alias() {
        let fields = [
            Field {
                name: "attackText".to_owned(),
                namespace: Some("MenuFXData".to_owned()),
                addr: 0x08fb,
                source_len: 1,
                span: 1,
                kind: FieldKind::Bytes,
                in_save: false,
                source: FieldSource::Unknown,
            },
            Field {
                name: "attackText".to_owned(),
                namespace: None,
                addr: 0x4db4a,
                source_len: 1,
                span: 1,
                kind: FieldKind::Bytes,
                in_save: false,
                source: FieldSource::Unknown,
            },
        ];
        let offsets = field_offsets(&fields);
        assert_eq!(offsets["attackText"], 0x08fb);
        assert_eq!(offsets["MenuFXData::attackText"], 0x08fb);
    }
}
