//! Machine-readable inventory for a complete project generation run.

use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// One generated artifact and the input that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub source: String,
}

/// Contents of `manifest.json` in a project's generated-artifact directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub tool_version: String,
    pub generated_at: String,
    pub artifacts: Vec<Artifact>,
}

/// A generated file and the source input responsible for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactInput {
    pub path: String,
    pub file: PathBuf,
    pub source: PathBuf,
}

impl ArtifactInput {
    /// Creates an input for a file immediately inside `output_dir`.
    pub fn in_output(output_dir: &Path, path: &str, source: &Path) -> Self {
        Self {
            path: path.to_owned(),
            file: output_dir.join(path),
            source: source.to_owned(),
        }
    }

    /// Creates an input for a generated file outside `output_dir`.
    pub fn external(file: &Path, source: &Path) -> Self {
        Self {
            path: file.to_string_lossy().into_owned(),
            file: file.to_owned(),
            source: source.to_owned(),
        }
    }
}

/// Writes a complete replacement manifest for the supplied generated files.
///
/// The manifest itself is intentionally excluded: including its own digest
/// would be self-referential.
pub fn emit(output_dir: &Path, artifacts: &[ArtifactInput]) -> anyhow::Result<()> {
    let artifacts = artifacts
        .iter()
        .map(|input| {
            let contents = std::fs::read(&input.file)
                .with_context(|| format!("Reading generated artifact {}", input.file.display()))?;
            Ok(Artifact {
                path: input.path.clone(),
                bytes: contents.len() as u64,
                sha256: format!("{:x}", Sha256::digest(contents)),
                source: input.source.to_string_lossy().into_owned(),
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let manifest = Manifest {
        tool_version: format!("cgfx-tools {}", env!("CARGO_PKG_VERSION")),
        generated_at: Utc::now().to_rfc3339(),
        artifacts,
    };
    let json = serde_json::to_vec_pretty(&manifest).context("Serializing artifact manifest")?;
    let path = output_dir.join("manifest.json");
    std::fs::write(&path, json).with_context(|| format!("Writing {}", path.display()))
}
