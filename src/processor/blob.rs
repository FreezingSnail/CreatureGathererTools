//! Minimal VM assembler stub.
//! Real assembler will go through lexer/AST/linker; for now we just
//! echo the plain text back as bytes so that the pipeline compiles.

use anyhow::{Result, anyhow};

use crate::model::ParsedScripts;
use crate::processor::ast::ToBytecode; // bring the trait into scope

#[derive(Debug)]
pub struct ProcessedScripts {
    pub blob: Vec<Vec<u8>>, // concatenated bytecode for all scripts
    pub offsets: Vec<u16>,  // starting offset of each script
}

/// Convert every raw script into “bytecode”.
pub fn assemble_scripts(parsed_scripts: &ParsedScripts) -> Result<ProcessedScripts> {
    let mut blob = Vec::<Vec<u8>>::new(); // final buffer (all chunks)
    let mut offsets = Vec::<u16>::new(); // absolute offsets into `blob`

    // Iterate over map-chunks (0‥2047)
    for (chunk_idx, chunk) in parsed_scripts.chunks.iter().enumerate() {
        // ------- assemble this chunk into a temporary buffer -------------
        let mut tmp = Vec::<u8>::new();
        let base_offset = blob.len() as u16; // where this chunk will start

        for script in chunk {
            for cmd in &script.body {
                // record absolute offset (base + current tmp len)
                offsets.push(base_offset + tmp.len() as u16);

                // encode command and append stub terminator
                tmp.extend_from_slice(&cmd.to_bytes());
                tmp.push(0x00);
            }
        }

        // ------- size check ----------------------------------------------
        if tmp.len() > 128 {
            return Err(anyhow!(
                "chunk {} too large, {} bytes instead of 128",
                chunk_idx,
                tmp.len()
            ));
        }

        // append verified chunk to the final blob
        blob.push(tmp);
    }

    Ok(ProcessedScripts { blob, offsets })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ScriptEntry, ScriptLayer};
    use crate::processor::script_parser::parse_scripts;

    /// Helper: parse a layer and immediately assemble it.
    fn pipe(layer: ScriptLayer) -> (ParsedScripts, ProcessedScripts) {
        let parsed = parse_scripts(&layer).expect("parser ok");
        let processed = assemble_scripts(&parsed).expect("assembler ok");
        (parsed, processed)
    }

    #[test]
    fn test_assemble_multiple_scripts_same_chunk() {
        // Two tiny scripts whose (x, y) both map to chunk 0.
        let layer = ScriptLayer {
            objects: vec![
                ScriptEntry {
                    id: 0,
                    script: "msg {a};".into(),
                    x: 1.0,
                    y: 1.0, // inside chunk 0
                },
                ScriptEntry {
                    id: 0,
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

        // Blob must be a Vec<Vec<u8>> with one chunk containing both scripts
        // Chunk 0: [0, 0, 0, 0, 0, 1, 0, 0]
        //          [Msg opcode(0) + text-idx 0 + terminator, Msg opcode(0) + text-idx 1 + terminator]
        assert_eq!(
            processed.blob.len(),
            1,
            "should have exactly 1 non-empty chunk"
        );
        assert_eq!(
            processed.blob[0],
            vec![
                0, 0, 0, 0, // first  script
                0, 1, 0, 0 // second script
            ]
        );
    }

    // ──────────────────────────────────────────────────────────────────
    //  Chunk-size limit
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn test_chunk_too_large_error() {
        // Each Msg encodes to 4 bytes (incl. the VM terminator),
        // so 129 messages → 516 bytes > 512.
        let mut scripts = Vec::<ScriptEntry>::new();
        for _ in 0..129 {
            scripts.push(ScriptEntry {
                id: 0,
                script: "msg {x};".into(),
                x: 0.0,
                y: 0.0, // all go into chunk 0
            });
        }
        let layer = ScriptLayer { objects: scripts };

        let parsed = parse_scripts(&layer).expect("parse ok");
        let err = assemble_scripts(&parsed).unwrap_err();

        assert!(
            err.to_string().starts_with("chunk 0 too large"),
            "got error message: {err}"
        );
    }
}
