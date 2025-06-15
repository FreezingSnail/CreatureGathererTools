//! Dump raw VM bytecode blob (stub).

use crate::model::ProcessedProject;
use std::fs;
use std::io;
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join(format!("{}_scripts.bin", "world"));
    fs::write(path, &project.vm.blob)?;
    Ok(())
}
