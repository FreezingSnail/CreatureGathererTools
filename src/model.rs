//! Simple shared structs that *both* parser and processor can touch.
//! We will extend these later.

use serde::{Deserialize, Serialize};

/// Immediately-after-parse representation (raw, 1-to-1 with JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawProject {
    /// For now just a name and an optional list of script strings.
    pub name: String,
    pub scripts: Vec<RawScript>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawScript {
    pub name: String,
    pub source: String,
}

/// Fully processed output handed to `writer`.
/// (Will contain map, tiles, etc. later; right now only the VM part.)
use crate::processor::vm;
pub struct ProcessedProject {
    pub name: String,
    pub vm: vm::ProcessedScripts,
}