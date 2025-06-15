//! Minimal VM assembler stub.
//! Real assembler will go through lexer/AST/linker; for now we just
//! echo the plain text back as bytes so that the pipeline compiles.

use crate::model::ScriptEntry;
use anyhow::Result;

pub struct ProcessedScripts {
    pub blob: Vec<u8>,     // concatenated bytecode for all scripts
    pub offsets: Vec<u16>, // starting offset of each script
    pub names: Vec<String>,
}

/// Convert every raw script into “bytecode”.
pub fn assemble_all(scripts: &Vec<ScriptEntry>) -> Result<ProcessedScripts> {
    let mut blob = Vec::<u8>::new();
    let mut offsets = Vec::<u16>::new();

    for s in scripts {
        // record offset before we push bytes
        offsets.push(blob.len() as u16);

        // Stub assembler: for now just copy UTF-8 bytes, plus a terminating 0
        blob.extend_from_slice(s.script.as_bytes());
        blob.push(0x00);
    }

    Ok(ProcessedScripts {
        blob,
        offsets,
        names: Vec::<String>::new(),
    })
}
