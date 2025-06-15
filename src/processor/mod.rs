//! Component 3 – the functional core.
//!
//! For the moment we only care about assembling scripts into byte-code.
pub mod ast;
pub mod lexer;
pub mod script_parser;
pub mod vm;

pub use crate::model::Script;

use crate::model::{ProcessedProject, RawProject};
use anyhow::Result;

/// Runs every processing pass and returns a read-only structure for writers.
pub fn run(raw: &RawProject) -> Result<ProcessedProject> {
    let vm_scripts = vm::assemble_all(&raw.scripts.objects)?;

    Ok(ProcessedProject { vm: vm_scripts })
}
