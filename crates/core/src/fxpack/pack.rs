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
    let mut current_namespace = None;
    for entry in &spec.entries {
        if let Source::Expand { path, exclude } = &entry.source {
            let declarations = carray::enumerate_expanded_declarations(path)
                .with_context(|| format!("enumerating expanded source {}", path.display()))?;
            let excluded = exclude.as_deref().unwrap_or_default();
            for expanded in declarations.into_iter().filter(|expanded| {
                !excluded
                    .iter()
                    .any(|name| name == &expanded.declaration.name)
            }) {
                let declaration = expanded.declaration;
                transition_namespace(
                    &mut layout,
                    &mut current_namespace,
                    declaration.namespace.as_deref(),
                )?;
                let offsets = field_offsets(layout.fields());
                let (bytes, kind, source) = match declaration.ctype {
                    carray::CType::U8 => (
                        declaration.bytes.ok_or_else(|| {
                            anyhow::anyhow!(
                                "expanded source {} declaration {:?} on line {} has no byte initializer",
                                path.display(),
                                declaration.name,
                                declaration.line,
                            )
                        })?,
                        FieldKind::Bytes,
                        match expanded.source {
                            carray::DeclarationSource::CArray => {
                                FieldSource::CArray { path: path.clone() }
                            }
                            carray::DeclarationSource::Symbol => {
                                FieldSource::Symbol { path: path.clone() }
                            }
                        },
                    ),
                    carray::CType::U24 | carray::CType::U32 => (
                        symbol::extract_symbol_bytes(
                            path,
                            &declaration.name,
                            declaration.namespace.as_deref(),
                            &offsets,
                        )
                        .with_context(|| {
                            format!(
                                "resolving expanded declaration {:?} at {}:{}",
                                declaration.name,
                                path.display(),
                                declaration.line,
                            )
                        })?,
                        FieldKind::Bytes,
                        FieldSource::Symbol { path: path.clone() },
                    ),
                };
                layout.field_with_source(&declaration.name, kind, source)?;
                layout.write(&bytes)?;
            }
            continue;
        }

        let name = entry
            .name
            .as_deref()
            .expect("validated non-expand entries require names");
        transition_namespace(
            &mut layout,
            &mut current_namespace,
            entry.namespace.as_deref(),
        )?;
        if let Some(align) = entry.align {
            layout.pad(align, false);
        }
        let offsets = field_offsets(layout.fields());
        let (bytes, kind) = resolve_entry(entry, builders, &offsets)
            .with_context(|| format!("resolving entry {name:?}"))?;
        layout.field_with_source(name, kind, source_metadata(&entry.source))?;
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

fn transition_namespace(
    layout: &mut Layout,
    current_namespace: &mut Option<String>,
    namespace: Option<&str>,
) -> anyhow::Result<()> {
    if current_namespace.as_deref() != namespace {
        if current_namespace.is_some() {
            layout.end_namespace()?;
        }
        if let Some(namespace) = namespace {
            layout.begin_namespace(namespace);
        }
        *current_namespace = namespace.map(str::to_owned);
    }
    Ok(())
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
        Source::Expand { .. } => unreachable!("expand entries are rejected before packing"),
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
        Source::Expand { .. } => {
            bail!("expand sources cannot be packed until expansion is implemented")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::field_offsets;
    use crate::fxpack::layout::{Field, FieldKind, FieldSource};

    fn field(name: &str, namespace: Option<&str>, addr: u32) -> Field {
        Field {
            name: name.to_owned(),
            namespace: namespace.map(str::to_owned),
            addr,
            source_len: 1,
            span: 1,
            kind: FieldKind::Bytes,
            in_save: false,
            source: FieldSource::Unknown,
        }
    }

    #[test]
    fn unqualified_offsets_keep_the_first_declaration_alias() {
        let fields = [
            field("attackText", Some("MenuFXData"), 0x08fb),
            field("attackText", None, 0x4db4a),
        ];
        let offsets = field_offsets(&fields);
        assert_eq!(offsets["attackText"], 0x08fb);
        assert_eq!(offsets["MenuFXData::attackText"], 0x08fb);
    }

    #[test]
    fn menu_string_collision_aliases_keep_the_first_declaration() {
        // Immutable project expectation: MenuStrings resolves its bare names
        // through the first MenuFXData declaration, not the later global one.
        const COLLISIONS: [(&str, u32, u32); 28] = [
            ("attackText", 0x0008fb, 0x04db4a),
            ("enemyAttackText", 0x00090c, 0x04b750),
            ("damageText", 0x000919, 0x04be58),
            ("hpText", 0x000926, 0x04b282),
            ("atkText", 0x00092b, 0x04b8f4),
            ("defText", 0x000931, 0x04bb4c),
            ("spdText", 0x000937, 0x04a166),
            ("satkText", 0x00093d, 0x0498ba),
            ("sdefText", 0x000944, 0x049662),
            ("pointerText", 0x00094b, 0x04daf0),
            ("Fainted", 0x000981, 0x04ad78),
            ("SwitchIn", 0x000989, 0x04d2da),
            ("win", 0x00099a, 0x04c0b0),
            ("lose", 0x0009a2, 0x04a7ba),
            ("escaped", 0x0009ad, 0x04cbb4),
            ("changedIn", 0x0009b9, 0x04c524),
            ("quarter", 0x0009cd, 0x04e55e),
            ("half", 0x0009df, 0x04b066),
            ("doubled", 0x0009ec, 0x04d8b6),
            ("quad", 0x0009fa, 0x04cf92),
            ("damage", 0x000a0c, 0x048744),
            ("physical", 0x000a13, 0x04bffc),
            ("special", 0x000a18, 0x0485dc),
            ("power", 0x000a1d, 0x04d6d6),
            ("move", 0x000a24, 0x04dd66),
            ("change", 0x000a29, 0x04e84c),
            ("escape", 0x000a30, 0x04e036),
            ("gather", 0x000a37, 0x04a3fa),
        ];
        let fields: Vec<Field> = COLLISIONS
            .iter()
            .flat_map(|(name, first, competing)| {
                [
                    field(name, Some("MenuFXData"), *first),
                    field(name, None, *competing),
                ]
            })
            .collect();
        let offsets = field_offsets(&fields);

        for (name, first, competing) in COLLISIONS {
            assert_eq!(
                offsets[name], first,
                "alias {name}: expected MenuFXData::{name}=0x{first:06X} before competing global {name}=0x{competing:06X}",
            );
            assert_eq!(
                offsets[&format!("MenuFXData::{name}")],
                first,
                "qualified MenuFXData::{name} changed from 0x{first:06X}; competing global {name}=0x{competing:06X}",
            );
        }
    }
}
