//! Public façade for “component 1”.

use crate::model::{RawProject, RawScript};
use anyhow::Result;

/// Parse the whole input JSON string into `RawProject`.
pub fn load_from_json(json: &str) -> Result<RawProject> {
    // Real code will understand Tiled's schema; for now deserialize blindly.
    let proj: RawProject = serde_json::from_str(json)?;
    Ok(proj)
}
