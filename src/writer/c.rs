//! Emit C++ header/source for the VM part without using external crates.
use crate::model::ProcessedProject;
use crate::processor::ast::Cmd;
use crate::processor::blob::ProcessedScripts;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    println!("writing opcodes");
    opcode_header(out_dir)?;
    println!("writing flags");
    flags(&project.flags, out_dir)?;
    println!("writing locations");
    locations(&project.locations, out_dir)?;
    println!("writing scripts");
    scripts(&project.blob, out_dir)?;
    Ok(())
}

fn opcode_header(out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("opcodes.hpp"))?;

    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <stdint.h>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;

    // ---------------------------------------------------------------
    // 1. Enum-class for opcodes – derived from Cmd::VARIANT_NAMES
    // ---------------------------------------------------------------
    writeln!(h, "enum class VmOpcode : uint8_t {{")?;
    for (idx, name) in Cmd::VARIANT_NAMES.iter().enumerate() {
        if *name == "End" {
            writeln!(h, "    {} = 255,", name)?;
        } else {
            writeln!(h, "    {} = {},", name, idx)?;
        }
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
    let mut h = File::create(out_dir.join("flag_bit_array.hpp"))?;
    let bits = flags.len() as u16;
    let bytes = bits / 8 + (bits % 8) as u16;

    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <stdint.h>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;
    writeln!(h, "extern uint8_t FLAG_BIT_ARRAY[{}];", bytes)?;

    let mut h = File::create(out_dir.join("flag_bit_array.cpp"))?;
    let bits = flags.len() as u16;
    let bytes = bits / 8 + (bits % 8) as u16;

    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <stdint.h>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;
    writeln!(h, "uint8_t FLAG_BIT_ARRAY[{}] = {{0}};", bytes)?;

    Ok(())
}

fn flag_names(flags: &HashMap<String, u16>, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("flags.hpp"))?;
    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <stdint.h>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;

    for (name, i) in flags {
        writeln!(h, "uint16_t {name} = {i};")?;
    }

    Ok(())
}

fn locations(locs: &HashMap<String, u16>, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("locations.hpp"))?;
    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <stdint.h>")?;
    writeln!(h, "// Auto-generated – DO NOT EDIT\n")?;
    for (name, i) in locs {
        writeln!(h, "uint16_t {name} = {i};")?;
    }

    Ok(())
}

fn scripts(blob: &ProcessedScripts, out_dir: &Path) -> io::Result<()> {
    let mut h = File::create(out_dir.join("scripts.hpp"))?;
    writeln!(h, "#pragma once")?;
    writeln!(h, "#include <stdint.h>")?;
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
        writeln!(h, "uint8_t blob{i}[] = {{ {} }};", joined)?;
    }

    Ok(())
}
