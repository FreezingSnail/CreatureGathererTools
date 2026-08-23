use anyhow::{Context, bail};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::layout::SAVE_SECTOR;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum CArraySpec {
    /// A C-array file whose `uint8_t` initializers are all extracted.
    Simple(PathBuf),
    /// One named `uint8_t` declaration extracted from a C-array file.
    Filtered { path: PathBuf, symbol: String },
}

impl CArraySpec {
    pub fn path(&self) -> &Path {
        match self {
            Self::Simple(path) | Self::Filtered { path, .. } => path,
        }
    }

    pub fn path_mut(&mut self) -> &mut PathBuf {
        match self {
            Self::Simple(path) | Self::Filtered { path, .. } => path,
        }
    }

    pub fn symbol(&self) -> Option<&str> {
        match self {
            Self::Simple(_) => None,
            Self::Filtered { symbol, .. } => Some(symbol),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Source {
    #[serde(rename = "raw")]
    Raw(PathBuf),
    #[serde(rename = "carray")]
    CArray(CArraySpec),
    #[serde(rename = "image")]
    Image {
        path: PathBuf,
        width: u16,
        height: u16,
        shades: u8,
        #[serde(default)]
        spacing: u8,
    },
    #[serde(rename = "builder")]
    Builder(String),
    /// Extract one namespace-qualified declaration from the isolated legacy oracle.
    #[serde(rename = "legacy")]
    Legacy(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    pub namespace: Option<String>,
    pub source: Source,
    pub align: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveEntry {
    pub name: String,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveSpec {
    pub sectors: u8,
    #[serde(default, rename = "entry")]
    pub entries: Vec<SaveEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutSpec {
    #[serde(default, rename = "entry")]
    pub entries: Vec<Entry>,
    pub save: Option<SaveSpec>,
}

impl LayoutSpec {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("reading fx layout {}", path.display()))?;
        let mut spec: Self = toml::from_str(&contents)
            .with_context(|| format!("parsing fx layout {}", path.display()))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        spec.resolve_paths(base);
        spec.validate(base)?;
        Ok(spec)
    }

    pub fn builder_keys(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.source {
                Source::Builder(key) => Some(key.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Namespace-qualified symbols that require the transitional legacy bridge.
    pub fn legacy_symbols(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.source {
                Source::Legacy(symbol) => Some(symbol.as_str()),
                _ => None,
            })
            .collect()
    }

    fn resolve_paths(&mut self, base: &Path) {
        for entry in &mut self.entries {
            match &mut entry.source {
                Source::Raw(path) => resolve_path(path, base),
                Source::CArray(spec) => resolve_path(spec.path_mut(), base),
                Source::Image { path, .. } => resolve_path(path, base),
                Source::Builder(_) | Source::Legacy(_) => {}
            }
        }
    }

    fn validate(&self, base: &Path) -> anyhow::Result<()> {
        let mut names = HashSet::new();
        for entry in &self.entries {
            validate_identifier("entry", &entry.name)?;
            if !names.insert(entry.name.as_str()) {
                bail!("duplicate entry name {:?}", entry.name);
            }
            if let Some(namespace) = &entry.namespace {
                validate_namespace_path(namespace)
                    .with_context(|| format!("entry {:?}", entry.name))?;
            }
            if let Some(align) = entry.align
                && !align.is_power_of_two()
            {
                bail!(
                    "entry {:?}: align {align} must be a non-zero power of two",
                    entry.name
                );
            }
            match &entry.source {
                Source::Raw(path) => validate_source_path(&entry.name, "raw", path, base)?,
                Source::CArray(spec) => {
                    validate_source_path(&entry.name, "carray", spec.path(), base)?
                }
                Source::Image { path, shades, .. } => {
                    validate_source_path(&entry.name, "image", path, base)?;
                    if !matches!(shades, 2..=4) {
                        bail!(
                            "entry {:?}: image shades {shades} must be 2, 3, or 4",
                            entry.name
                        );
                    }
                }
                Source::Builder(_) => {}
                Source::Legacy(symbol) => {
                    if symbol.is_empty() || symbol.split("::").any(|part| !is_identifier(part)) {
                        bail!(
                            "entry {:?}: legacy symbol {symbol:?} must be a namespace-qualified C++ identifier",
                            entry.name
                        );
                    }
                }
            }
        }

        if let Some(save) = &self.save {
            if save.sectors == 0 {
                bail!("save sectors must be at least 1");
            }
            let save_len = u32::from(save.sectors) * SAVE_SECTOR;
            let mut previous_offset = None;
            for entry in &save.entries {
                validate_identifier("save entry", &entry.name)?;
                if !names.insert(entry.name.as_str()) {
                    bail!("duplicate entry name {:?}", entry.name);
                }
                if entry.offset % SAVE_SECTOR != 0 {
                    bail!(
                        "save entry {:?}: offset {} must be aligned to {SAVE_SECTOR}",
                        entry.name,
                        entry.offset
                    );
                }
                if entry.offset >= save_len {
                    bail!(
                        "save entry {:?}: offset {} is outside {} save sectors",
                        entry.name,
                        entry.offset,
                        save.sectors
                    );
                }
                if let Some(previous_offset) = previous_offset
                    && entry.offset <= previous_offset
                {
                    bail!(
                        "save entry {:?}: offset {} must be ascending after {previous_offset}",
                        entry.name,
                        entry.offset
                    );
                }
                previous_offset = Some(entry.offset);
            }
        }

        Ok(())
    }
}

fn resolve_path(path: &mut PathBuf, base: &Path) {
    if path.is_relative() {
        *path = base.join(&*path);
    }
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn validate_namespace_path(path: &str) -> anyhow::Result<()> {
    if path.split("::").all(is_identifier) {
        return Ok(());
    }
    bail!(
        "namespace path {path:?} must contain non-empty C++ identifier components separated by ::"
    );
}

fn validate_identifier(kind: &str, value: &str) -> anyhow::Result<()> {
    if !is_identifier(value) {
        bail!("{kind} name {value:?} must be a non-empty C++ identifier");
    }
    Ok(())
}

fn validate_source_path(name: &str, kind: &str, path: &Path, base: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        bail!(
            "entry {name:?}: {kind} source {} does not exist (relative to {})",
            path.display(),
            base.display()
        );
    }
    Ok(())
}
