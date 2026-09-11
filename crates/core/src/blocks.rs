//! Project-level tile block library.
//!
//! Blocks are reusable rectangles of map tiles authored from the project
//! sprite sheet. They live in their own project file rather than inside a map
//! so the same library can be used as a map-editor brush and as WFC piece
//! input. Tiles use the same run-length encoding as WFC pieces and stamps
//! (`crates/core/src/wfcmap/rle.rs`).

use serde::{Deserialize, Serialize};

use crate::wfcmap::rle;

/// File format version written to the blocks file.
pub const BLOCK_LIBRARY_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub w: u32,
    #[serde(default)]
    pub h: u32,
    /// Row-major tile ids, run-length encoded.
    #[serde(default)]
    pub tiles: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl Block {
    /// Decoded row-major tile ids, or an error naming the offending block.
    pub fn tile_values(&self) -> anyhow::Result<Vec<u16>> {
        let expected = (self.w as usize) * (self.h as usize);
        rle::decode(&self.tiles, expected)
            .map_err(|e| anyhow::anyhow!("block {} ({}): {e}", self.id, self.name))
    }
}

/// On-disk shape of the blocks file. Wrapped in an object so the format can
/// gain project-level fields without breaking existing libraries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockLibrary {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

fn default_version() -> u32 {
    BLOCK_LIBRARY_VERSION
}

impl Default for BlockLibrary {
    fn default() -> Self {
        BlockLibrary {
            version: BLOCK_LIBRARY_VERSION,
            blocks: Vec::new(),
        }
    }
}

impl BlockLibrary {
    pub fn new(blocks: Vec<Block>) -> Self {
        BlockLibrary {
            version: BLOCK_LIBRARY_VERSION,
            blocks,
        }
    }

    /// Reports every structural problem together: duplicate ids, empty extents
    /// and tile payloads that do not match `w * h`.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let mut seen: Vec<u32> = Vec::new();
        for block in &self.blocks {
            let label = if block.name.is_empty() {
                format!("block {}", block.id)
            } else {
                format!("block {} ({})", block.id, block.name)
            };
            if seen.contains(&block.id) {
                errors.push(format!("duplicate block id {}", block.id));
            }
            seen.push(block.id);
            if block.w == 0 || block.h == 0 {
                errors.push(format!("{label} has empty extent {}x{}", block.w, block.h));
                continue;
            }
            if let Err(e) = block.tile_values() {
                errors.push(e.to_string());
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: u32, w: u32, h: u32, tiles: &str) -> Block {
        Block {
            id,
            name: format!("block {id}"),
            w,
            h,
            tiles: tiles.to_string(),
            tags: vec![],
        }
    }

    #[test]
    fn decodes_run_length_tiles() {
        let values = block(1, 2, 2, "4x3,9").tile_values().expect("must decode");
        assert_eq!(values, vec![4, 4, 4, 9]);
    }

    #[test]
    fn missing_library_fields_default() {
        let library: BlockLibrary = serde_json::from_str("{}").expect("must parse");
        assert_eq!(library.version, BLOCK_LIBRARY_VERSION);
        assert!(library.blocks.is_empty());
    }

    #[test]
    fn validate_reports_all_problems_together() {
        let library = BlockLibrary::new(vec![
            block(1, 2, 2, "1x4"),
            block(1, 0, 3, ""),
            block(2, 2, 2, "1x3"),
        ]);
        let errors = library.validate().expect_err("invalid library must fail");
        assert!(errors.iter().any(|e| e.contains("duplicate block id 1")));
        assert!(errors.iter().any(|e| e.contains("empty extent 0x3")));
        assert!(errors.iter().any(|e| e.contains("block 2")));
    }

    #[test]
    fn validate_accepts_a_consistent_library() {
        let library = BlockLibrary::new(vec![block(1, 2, 2, "1x4"), block(2, 1, 3, "5,6,7")]);
        assert!(library.validate().is_ok());
    }

    #[test]
    fn tags_are_omitted_when_empty() {
        let json = serde_json::to_string(&block(3, 1, 1, "0")).expect("must serialize");
        assert!(!json.contains("tags"), "empty tags must not be written");
    }
}
