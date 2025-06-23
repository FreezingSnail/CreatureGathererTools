//! Dump raw VM bytecode blob (stub).

use crate::model::ProcessedProject;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    scripts(project, out_dir)?;
    map(project, out_dir)?;
    Ok(())
}

fn scripts(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("scripts.bin");
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    for blob_chunk in &project.blob.blob {
        // Write the actual blob data
        writer.write_all(&blob_chunk.blob)?;

        // Calculate padding needed for 128-byte alignment
        let padding = 128 - (blob_chunk.blob.len() % 128);
        if padding < 128 {
            // Write all padding bytes at once instead of one by one
            let padding_bytes = vec![0u8; padding];
            writer.write_all(&padding_bytes)?;
        }
    }

    writer.flush()?;
    Ok(())
}

fn map(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("map.bin");
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    for chunk in &project.map {
        // Write the actual blob data
        for &value in chunk {
            writer.write_all(&value.to_be_bytes())?;
        }
    }

    writer.flush()?;
    Ok(())
}
