//! Dump raw VM bytecode blob (stub).

use crate::model::ProcessedProject;
use std::fs;
use std::io;
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    scripts(project, out_dir)?;
    Ok(())
}

fn scripts(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("scripts.bin");
    for blob in &project.blob.blob {
        fs::write(&path, &blob.blob)?;
        // write n emtpy bytes to pad to 128 bytes alignment
        let padding = 128 - (blob.blob.len() % 128);
        for _ in 0..padding {
            fs::write(&path, &[0])?;
        }
    }
    Ok(())
}
