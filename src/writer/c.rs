//! Emit C++ header/source for the VM part without using external crates.

use crate::model::ProcessedProject;
use crate::processor::ast::Cmd;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join(format!("{}_scripts.h", project.name)))?;

    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <cstdint>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;

    // ---------------------------------------------------------------
    // 1. Enum-class for opcodes – derived from Cmd::VARIANT_NAMES
    // ---------------------------------------------------------------
    writeln!(h, "enum class VmOpcode : uint8_t {{")?;
    for (idx, name) in Cmd::VARIANT_NAMES.iter().enumerate() {
        writeln!(h, "    {} = {},", name, idx)?;
    }
    writeln!(h, "}};\n")?;

    // ---------------------------------------------------------------
    // 2. Script blob & size symbols
    // ---------------------------------------------------------------
    writeln!(
        h,
        "extern const uint8_t  {n}_scripts_bin[];",
        n = project.name
    )?;
    writeln!(
        h,
        "extern const uint32_t {n}_scripts_size;",
        n = project.name
    )?;

    Ok(())
}
