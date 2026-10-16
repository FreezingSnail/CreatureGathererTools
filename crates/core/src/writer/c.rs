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
    std::fs::write(out_dir.join("opcodes.hpp"), opcode_header_text())
}

/// `enum class VmOpcode` derived from `Cmd::VARIANT_NAMES`: the array index is
/// the numeric opcode (with `End` pinned to 255). Adding a command to the
/// canonical list is what appends its opcode here.
fn opcode_header_text() -> String {
    let mut out = String::from(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n",
    );
    out.push_str("enum class VmOpcode : uint8_t {\n");
    for (idx, name) in Cmd::VARIANT_NAMES.iter().enumerate() {
        if *name == "End" {
            out.push_str(&format!("    {} = 255,\n", name));
        } else {
            out.push_str(&format!("    {} = {},\n", name, idx));
        }
    }
    out.push_str("};\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opcode_header_appends_giveitem_nine() {
        let text = opcode_header_text();
        assert!(text.contains("    GiveItem = 9,\n"), "header: {text}");
        // Existing numbers and End stay pinned.
        assert!(text.contains("    ReadFlag = 8,\n"));
        assert!(text.contains("    Msg = 0,\n"));
        assert!(text.contains("    End = 255,\n"));
        // GiveItem is the final numbered enumerator before End.
        let give = text.find("GiveItem = 9").unwrap();
        let end = text.find("End = 255").unwrap();
        assert!(give < end);
    }
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
