//! Component 3 – the functional core.
//!
//! For the moment we only care about assembling scripts into byte-code.
pub mod ast;
pub mod blob;
pub mod lexer;
pub mod script_parser;

pub use crate::model::Script;

use crate::model::{ProcessedProject, RawProject};
use anyhow::Result;

/// Runs every processing pass and returns a read-only structure for writers.
pub fn run(raw: &RawProject) -> Result<ProcessedProject> {
    let parse_result = script_parser::parse_scripts(&raw.scripts);
    let processed = match parse_result {
        Ok(processed) => processed,
        Err(e) => {
            panic!("Error parsing scripts: {}", e);
        }
    };
    let vm_scripts = blob::assemble_scripts(&processed)?;

    Ok(ProcessedProject {
        vm: vm_scripts,
        flags: processed.flags,
        locations: processed.tags,
        texts: processed.texts,
    })
}
