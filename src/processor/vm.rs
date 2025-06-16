//! Minimal VM assembler stub.
//! Real assembler will go through lexer/AST/linker; for now we just
//! echo the plain text back as bytes so that the pipeline compiles.

use anyhow::Result;

use crate::model::ParsedScripts;
use crate::processor::ast::ToBytecode; // ← bring the trait into scope
use crate::processor::script_parser::parse_scripts; // ← needed for tests

pub struct ProcessedScripts {
    pub blob: Vec<u8>,     // concatenated bytecode for all scripts
    pub offsets: Vec<u16>, // starting offset of each script
    pub names: Vec<String>,
}

/// Convert every raw script into “bytecode”.
pub fn assemble_scripts(parsed_scripts: &ParsedScripts) -> Result<ProcessedScripts> {
    let mut blob = Vec::<u8>::new();
    let mut offsets = Vec::<u16>::new();

    for chunk in &parsed_scripts.chunks {
        for script in chunk {
            for cmd in &script.body {
                offsets.push(blob.len() as u16);
                blob.extend_from_slice(&cmd.to_bytes());
            }
            blob.push(0x00); // terminator (stub)
        }
    }

    Ok(ProcessedScripts {
        blob,
        offsets,
        names: Vec::<String>::new(),
    })
}

/* ------------------------------------------------------------------------- */
/*  Unit-tests                                                              */
/* ------------------------------------------------------------------------- */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ScriptEntry, ScriptLayer};

    /// Helper: parse a layer and immediately assemble it.
    fn pipe(layer: ScriptLayer) -> (ParsedScripts, ProcessedScripts) {
        let parsed = parse_scripts(layer).expect("parser ok");
        let processed = assemble_scripts(&parsed).expect("assembler ok");
        (parsed, processed)
    }

    // ──────────────────────────────────────────────────────────────────
    //  Multiple scripts inside one chunk (chunk #0)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_assemble_multiple_scripts_same_chunk() {
        // Two tiny scripts whose (x, y) both map to chunk 0.
        let layer = ScriptLayer {
            objects: vec![
                ScriptEntry {
                    script: "msg {a};".into(),
                    x: 1.0,
                    y: 1.0, // inside chunk 0
                },
                ScriptEntry {
                    script: "msg {b};".into(),
                    x: 2.0,
                    y: 1.0, // same chunk 0
                },
            ],
        };

        let (parsed, processed) = pipe(layer);

        // ── Chunk grouping ───────────────────────────────────────────
        assert_eq!(parsed.chunks[0].len(), 2, "both scripts in chunk 0");

        // ── Offsets ──────────────────────────────────────────────────
        // Each Msg serialises to 3 bytes, assembler adds 0-terminator
        // → 4 bytes per command
        assert_eq!(processed.offsets, vec![0u16, 4u16]);

        // Blob must be:
        //  [0, 0, 0, 0]   Msg opcode(0) + text-idx 0 + terminator
        //  [0, 1, 0, 0]   Msg opcode(0) + text-idx 1 + terminator
        assert_eq!(
            processed.blob,
            vec![
                0, 0, 0, 0, // first  script
                0, 1, 0, 0 // second script
            ]
        );
    }
}
