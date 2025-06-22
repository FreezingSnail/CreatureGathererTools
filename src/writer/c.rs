//! Emit C++ header/source for the VM part without using external crates.
use crate::model::ProcessedProject;
use crate::processor::ast::Cmd;
use crate::processor::blob::ProcessedScripts;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    opcode_header(out_dir)?;
    flags(&project.flags, out_dir)?;
    locations(&project.locations, out_dir)?;
    scripts(&project.blob, out_dir)?;
    Ok(())
}

fn opcode_header(out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("opcodes.h"))?;

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

    Ok(())
}

fn flags(flags: &HashMap<String, u16>, out_dir: &Path) -> io::Result<()> {
    flag_bit_arr(flags, out_dir)?;
    flag_names(flags, out_dir)?;
    Ok(())
}

fn flag_bit_arr(flags: &HashMap<String, u16>, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("flag_bit_array.h"))?;
    let bits = flags.len() as u16;
    let bytes = bits / 8 + (bits % 8) as u16;

    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <cstdint>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;
    writeln!(h, "const uint8_t FLAG_BIT_ARRAY[{}];", bytes)?;

    Ok(())
}

fn flag_names(flags: &HashMap<String, u16>, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("flags.h"))?;
    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <cstdint>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;

    for (name, i) in flags {
        writeln!(h, "constexpr uint16_t {name} = {i};")?;
    }

    Ok(())
}

fn locations(locs: &HashMap<String, u16>, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("locations.h"))?;
    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <cstdint>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;
    for (name, i) in locs {
        writeln!(h, "constexpr uint16_t {name} = {i};")?;
    }

    Ok(())
}

fn scripts(blob: &ProcessedScripts, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("scripts.h"))?;
    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <cstdint>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;
    for (i, blob) in blob.blob.iter().enumerate() {
        if blob.blob.len() <= 1 {
            continue;
        }
        let str_nums: Vec<String> = blob
            .blob
            .iter()
            .map(|n| n.to_string()) // Convert each u8 to a string
            .collect(); // Collect into a Vec<String>
        let joined = str_nums.join(",");
        writeln!(h, "// {}", blob.script)?;
        writeln!(h, "constexpr uint8_t blob{i}[] = {{ {} }};", joined)?;
    }

    Ok(())
}
