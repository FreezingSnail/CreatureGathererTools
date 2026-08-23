use anyhow::{Context, bail};
use std::path::Path;

pub struct PackParityInputs<'a> {
    pub fxdata_txt: &'a Path,
    pub python_script: &'a Path,
    pub work_dir: &'a Path,
}

/// Verify the Rust packer output matches fxdata-build.py byte for byte.
pub fn verify_pack_parity(
    inputs: &PackParityInputs<'_>,
    rust_bin: &[u8],
    rust_header: &str,
    rust_command: &str,
) -> anyhow::Result<()> {
    let (python_bin, python_header) = run_python_packer(inputs)?;

    if let Some((offset, rust_byte, python_byte)) = first_difference(rust_bin, &python_bin) {
        let window = hex_window(rust_bin, &python_bin, offset);
        bail!(
            "fxdata.bin mismatch at offset 0x{offset:06X}: \
             rust=0x{rust_byte:02X} python=0x{python_byte:02X}\n\
             {window}\n\
             Reproduce: {rust_command}"
        );
    }
    if rust_bin.len() != python_bin.len() {
        bail!(
            "fxdata.bin length mismatch: rust={} python={}\n\
             Reproduce: {rust_command}",
            rust_bin.len(),
            python_bin.len()
        );
    }

    let rust_lines = normalize_header(rust_header);
    let python_lines = normalize_header(&python_header);
    if rust_lines != python_lines {
        for (i, (r, p)) in rust_lines.iter().zip(python_lines.iter()).enumerate() {
            if r != p {
                bail!(
                    "fxdata.h mismatch at line {i}:\n  rust:   {r}\n  python: {p}\n\
                     Reproduce: {rust_command}"
                );
            }
        }
        bail!(
            "fxdata.h line count differs: rust={} python={}\n\
             Reproduce: {rust_command}",
            rust_lines.len(),
            python_lines.len()
        );
    }

    Ok(())
}

fn run_python_packer(inputs: &PackParityInputs<'_>) -> anyhow::Result<(Vec<u8>, String)> {
    use std::process::Command;

    let python = "python3";
    let check = Command::new("which").arg(python).output();
    if check.is_err() || !check.unwrap().status.success() {
        bail!("python3 not found; cannot run parity gate");
    }

    if !inputs.python_script.exists() {
        bail!(
            "parity script not found: {}",
            inputs.python_script.display()
        );
    }
    let python_script = inputs
        .python_script
        .canonicalize()
        .with_context(|| format!("resolving {}", inputs.python_script.display()))?;

    let scratch = inputs.work_dir.join("parity_scratch");
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).ok();
    }
    let fxdata_dir = inputs
        .fxdata_txt
        .parent()
        .context("fxdata_txt has no parent directory")?;
    copy_dir_recursive(fxdata_dir, &scratch)
        .with_context(|| format!("copying {} to scratch", fxdata_dir.display()))?;

    let scratch_txt = scratch.join(
        inputs
            .fxdata_txt
            .file_name()
            .context("fxdata_txt has no filename")?,
    );
    let output = Command::new(python)
        .arg(&python_script)
        .arg(&scratch_txt)
        .current_dir(&scratch)
        .output()
        .with_context(|| format!("running {python} {}", python_script.display()))?;

    if !output.status.success() {
        bail!(
            "python packer failed (exit {}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let bin_path = scratch.join("fxdata.bin");
    let header_path = scratch.join("fxdata.h");
    let python_bin =
        std::fs::read(&bin_path).with_context(|| format!("reading {}", bin_path.display()))?;
    let python_header = std::fs::read_to_string(&header_path)
        .with_context(|| format!("reading {}", header_path.display()))?;

    std::fs::remove_dir_all(&scratch).ok();

    Ok((python_bin, python_header))
}

/// Normalize header: drop the generated-by banner and blank lines.
pub fn normalize_header(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| !line.starts_with("/****") || !line.ends_with("****/"))
        .filter(|line| !line.is_empty())
        .collect()
}

pub fn first_difference(a: &[u8], b: &[u8]) -> Option<(usize, u8, u8)> {
    let len = a.len().min(b.len());
    for i in 0..len {
        if a[i] != b[i] {
            return Some((i, a[i], b[i]));
        }
    }
    if a.len() > len {
        Some((len, a[len], 0))
    } else if b.len() > len {
        Some((len, 0, b[len]))
    } else {
        None
    }
}

fn hex_window(a: &[u8], b: &[u8], offset: usize) -> String {
    let start = offset.saturating_sub(8);
    let end_a = offset.saturating_add(8).min(a.len());
    let end_b = offset.saturating_add(8).min(b.len());
    let rust_hex: String = a[start..end_a]
        .iter()
        .map(|b| format!("{b:02X} "))
        .collect();
    let py_hex: String = b[start..end_b]
        .iter()
        .map(|b| format!("{b:02X} "))
        .collect();
    format!("  rust:   [{rust_hex}]\n  python: [{py_hex}]")
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}
