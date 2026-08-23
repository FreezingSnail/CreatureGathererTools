use crate::fxpack::layout::Offsets;
use anyhow::{Context, ensure};
use std::path::{Path, PathBuf};

pub struct Artifacts {
    pub data_bin: PathBuf,
    pub save_bin: Option<PathBuf>,
    pub dev_bin: PathBuf,
}

pub fn write(
    bytes: &[u8],
    offsets: &Offsets,
    out_dir: &Path,
    stem: &str,
) -> anyhow::Result<Artifacts> {
    let required = offsets.data_len_flash as usize + offsets.save_len_flash as usize;
    ensure!(
        bytes.len() >= required,
        "byte buffer is {} bytes but offsets require at least {} (data_len_flash={} + save_len_flash={})",
        bytes.len(),
        required,
        offsets.data_len_flash,
        offsets.save_len_flash
    );

    std::fs::create_dir_all(out_dir).with_context(|| format!("Creating {}", out_dir.display()))?;

    let data_bin = out_dir.join(format!("{stem}-data.bin"));
    std::fs::write(&data_bin, &bytes[..offsets.data_len as usize])
        .with_context(|| format!("Writing {}", data_bin.display()))?;

    let save_bin = if offsets.save_len > 0 {
        let save_start = offsets.data_len_flash as usize;
        let save_end = save_start + offsets.save_len as usize;
        let path = out_dir.join(format!("{stem}-save.bin"));
        std::fs::write(&path, &bytes[save_start..save_end])
            .with_context(|| format!("Writing {}", path.display()))?;
        Some(path)
    } else {
        None
    };

    let dev_bin = out_dir.join(format!("{stem}.bin"));
    let mut dev = Vec::with_capacity(required);
    dev.extend_from_slice(&bytes[..offsets.data_len_flash as usize]);
    if offsets.save_len_flash > 0 {
        let save_start = offsets.data_len_flash as usize;
        dev.extend_from_slice(&bytes[save_start..save_start + offsets.save_len_flash as usize]);
    }
    std::fs::write(&dev_bin, &dev).with_context(|| format!("Writing {}", dev_bin.display()))?;

    Ok(Artifacts {
        data_bin,
        save_bin,
        dev_bin,
    })
}
