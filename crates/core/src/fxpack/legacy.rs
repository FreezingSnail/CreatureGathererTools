//! Transitional bridge for legacy `fxdata-build.py` declarations.
//!
//! The native packer cannot yet resolve symbolic `uint24_t` initializers. This
//! module runs the legacy oracle in a disposable copy of the FX input tree and
//! extracts declared byte ranges from its `fxdata.h` and `fxdata-data.bin`.
//! Remove this bridge after native builders and address resolution cover every
//! legacy declaration.

use anyhow::{Context, bail};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

/// Inputs for one isolated invocation of the legacy FX data builder.
pub struct LegacyBridgeInputs<'a> {
    /// The legacy `fxdata.txt` input file. Its containing tree is copied.
    pub fxdata_txt: &'a Path,
    /// Legacy `fxdata-build.py` script, kept outside the copied input tree.
    pub python_script: &'a Path,
    /// Python executable, normally `python3`.
    pub python: &'a Path,
    /// Parent for the disposable scratch copy; never place it below `fxdata_txt`.
    pub scratch_root: &'a Path,
}

/// Extracted legacy data, keyed by fully-qualified declaration name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyOutput {
    symbols: BTreeMap<String, Vec<u8>>,
}

impl LegacyOutput {
    /// Bytes emitted for an exact namespace-qualified symbol.
    pub fn bytes(&self, symbol: &str) -> anyhow::Result<&[u8]> {
        self.symbols
            .get(symbol)
            .map(Vec::as_slice)
            .with_context(|| format!("legacy bridge did not extract symbol {symbol:?}"))
    }

    /// Deterministic, lexicographically ordered symbol names.
    pub fn symbols(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.symbols
            .iter()
            .map(|(symbol, bytes)| (symbol.as_str(), bytes.as_slice()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Declaration {
    symbol: String,
    address: usize,
}

/// Run `fxdata-build.py` in an isolated copy and extract requested symbols.
///
/// The scratch directory is removed whether the oracle succeeds or fails. The
/// source FX tree and its committed/generated outputs are never write targets.
pub fn extract(
    inputs: &LegacyBridgeInputs<'_>,
    symbols: &[String],
) -> anyhow::Result<LegacyOutput> {
    validate_inputs(inputs)?;
    if symbols.is_empty() {
        return Ok(LegacyOutput {
            symbols: BTreeMap::new(),
        });
    }

    let scratch = inputs.scratch_root.join("legacy-fxdata-build");
    if scratch.exists() {
        bail!(
            "legacy bridge scratch directory already exists: {}; remove it after confirming no other pack is running",
            scratch.display()
        );
    }
    let result = (|| {
        let source_dir = inputs
            .fxdata_txt
            .parent()
            .context("legacy fxdata input has no parent directory")?;
        let project_dir = source_dir.parent().context(
            "legacy fxdata input must live in a project fxdata directory so oracle-relative assets can be isolated",
        )?;
        ensure_scratch_outside_project(project_dir, inputs.scratch_root)?;
        copy_project_for_oracle(project_dir, &scratch).with_context(|| {
            format!(
                "copying {} into isolated bridge scratch",
                project_dir.display()
            )
        })?;
        let scratch_txt = scratch
            .join(
                source_dir
                    .file_name()
                    .context("legacy fxdata input directory has no name")?,
            )
            .join(
                inputs
                    .fxdata_txt
                    .file_name()
                    .context("legacy fxdata input has no filename")?,
            );
        let scratch_fxdata_dir = scratch_txt
            .parent()
            .context("legacy scratch fxdata input has no parent directory")?;
        run_oracle(inputs, &scratch_txt, scratch_fxdata_dir)?;
        read_requested_outputs(scratch_fxdata_dir, symbols)
    })();
    let cleanup = std::fs::remove_dir_all(&scratch)
        .with_context(|| format!("removing legacy bridge scratch {}", scratch.display()));
    match (result, cleanup) {
        (Ok(output), Ok(())) => Ok(output),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), _) => Err(error),
    }
}

fn validate_inputs(inputs: &LegacyBridgeInputs<'_>) -> anyhow::Result<()> {
    if !inputs.fxdata_txt.is_file() {
        bail!(
            "legacy bridge fxdata input not found: {}",
            inputs.fxdata_txt.display()
        );
    }
    if !inputs.python_script.is_file() {
        bail!(
            "legacy bridge script not found: {}",
            inputs.python_script.display()
        );
    }
    Command::new(inputs.python)
        .arg("--version")
        .output()
        .with_context(|| {
            format!(
                "legacy bridge Python executable not found: {}",
                inputs.python.display()
            )
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(())
            } else {
                bail!(
                    "legacy bridge Python executable failed (exit {}): {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr).trim()
                )
            }
        })
}

fn run_oracle(
    inputs: &LegacyBridgeInputs<'_>,
    scratch_txt: &Path,
    scratch: &Path,
) -> anyhow::Result<()> {
    let output = Command::new(inputs.python)
        .arg(inputs.python_script)
        .arg(scratch_txt)
        .current_dir(scratch)
        .output()
        .with_context(|| {
            format!(
                "running legacy bridge oracle {} {}",
                inputs.python.display(),
                inputs.python_script.display()
            )
        })?;
    if !output.status.success() {
        bail!(
            "legacy bridge oracle failed (exit {}):\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn read_requested_outputs(scratch: &Path, requested: &[String]) -> anyhow::Result<LegacyOutput> {
    let header_path = scratch.join("fxdata.h");
    let data_path = scratch.join("fxdata-data.bin");
    let header = std::fs::read_to_string(&header_path).with_context(|| {
        format!(
            "legacy bridge oracle did not produce {}",
            header_path.display()
        )
    })?;
    let data = std::fs::read(&data_path).with_context(|| {
        format!(
            "legacy bridge oracle did not produce {}",
            data_path.display()
        )
    })?;
    extract_from_oracle_outputs(&header, &data, requested)
}

/// Extract requested declarations from generated oracle outputs.
///
/// Declaration end is the next declaration address, or `FX_DATA_BYTES` (when
/// present), otherwise the full data binary length. Names are namespace-qualified
/// exactly as in `fxdata.h`, such as `EventData::eventTable`.
pub fn extract_from_oracle_outputs(
    header: &str,
    data: &[u8],
    requested: &[String],
) -> anyhow::Result<LegacyOutput> {
    let (mut declarations, data_end) = parse_declarations(header, data.len())?;
    declarations.sort_by_key(|declaration| declaration.address);

    let mut ranges = BTreeMap::new();
    for (index, declaration) in declarations.iter().enumerate() {
        let end = declarations
            .get(index + 1)
            .map(|next| next.address)
            .unwrap_or(data_end);
        if declaration.address > end || end > data_end {
            bail!(
                "legacy bridge declaration {:?} has invalid range 0x{:06X}..0x{:06X} for data length {data_end}",
                declaration.symbol,
                declaration.address,
                end
            );
        }
        ranges.insert(declaration.symbol.as_str(), &data[declaration.address..end]);
    }

    let mut output = BTreeMap::new();
    let requested: BTreeSet<String> = requested.iter().cloned().collect();
    for symbol in requested {
        let bytes = ranges.get(symbol.as_str()).with_context(|| {
            format!(
                "legacy bridge declaration {symbol:?} not found; use namespace-qualified names from generated fxdata.h"
            )
        })?;
        output.insert(symbol, bytes.to_vec());
    }
    Ok(LegacyOutput { symbols: output })
}

fn parse_declarations(
    header: &str,
    binary_len: usize,
) -> anyhow::Result<(Vec<Declaration>, usize)> {
    let mut namespaces = Vec::new();
    let mut declarations = Vec::new();
    let mut data_end = binary_len;
    let mut saw_data_end = false;

    for raw_line in header.lines() {
        let line = raw_line
            .split_once("//")
            .map_or(raw_line, |(code, _)| code)
            .trim();
        if line.is_empty() {
            continue;
        }
        if let Some(namespace) = parse_namespace(line) {
            namespaces.push(namespace.to_owned());
            continue;
        }
        let close_count = line.bytes().take_while(|byte| *byte == b'}').count();
        if close_count > 0 {
            for _ in 0..close_count {
                namespaces.pop();
            }
            continue;
        }
        let Some((name, value)) = parse_uint24_constant(line)? else {
            continue;
        };
        if name == "FX_DATA_BYTES" {
            data_end = value;
            saw_data_end = true;
            continue;
        }
        if name == "FX_SAVE_BYTES" {
            continue;
        }
        let symbol = if namespaces.is_empty() {
            name.to_owned()
        } else {
            format!("{}::{name}", namespaces.join("::"))
        };
        declarations.push(Declaration {
            symbol,
            address: value,
        });
    }
    if saw_data_end && data_end != binary_len {
        bail!(
            "legacy bridge FX_DATA_BYTES ({data_end}) does not match fxdata-data.bin length ({binary_len})"
        );
    }
    Ok((declarations, data_end))
}

fn parse_namespace(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("namespace")?.trim_start();
    let name_end = rest
        .bytes()
        .position(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_'))
        .unwrap_or(rest.len());
    let name = &rest[..name_end];
    (!name.is_empty()).then_some(name)
}

fn parse_uint24_constant(line: &str) -> anyhow::Result<Option<(&str, usize)>> {
    let Some(rest) = line.strip_prefix("constexpr uint24_t") else {
        return Ok(None);
    };
    let (name, value) = rest
        .trim_start()
        .split_once('=')
        .with_context(|| format!("legacy bridge malformed uint24_t declaration: {line:?}"))?;
    let name = name.trim();
    let value = value.trim().trim_end_matches(';').trim();
    if name.is_empty() || value.is_empty() {
        bail!("legacy bridge malformed uint24_t declaration: {line:?}");
    }
    let value = if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        usize::from_str_radix(hex, 16)
    } else {
        value.parse()
    }
    .with_context(|| format!("legacy bridge parsing uint24_t value {value:?}"))?;
    Ok(Some((name, value)))
}

fn ensure_scratch_outside_project(project_dir: &Path, scratch_root: &Path) -> anyhow::Result<()> {
    let project_dir = project_dir.canonicalize().with_context(|| {
        format!(
            "resolving legacy project directory {}",
            project_dir.display()
        )
    })?;
    let scratch_root = if scratch_root.is_absolute() {
        scratch_root.to_owned()
    } else {
        std::env::current_dir()
            .context("resolving current directory for legacy bridge scratch")?
            .join(scratch_root)
    };
    if scratch_root.starts_with(&project_dir) {
        bail!(
            "legacy bridge scratch root {} must be outside copied project {}",
            scratch_root.display(),
            project_dir.display()
        );
    }
    Ok(())
}

fn copy_project_for_oracle(source: &Path, destination: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(destination)
        .with_context(|| format!("creating {}", destination.display()))?;
    for entry in
        std::fs::read_dir(source).with_context(|| format!("reading {}", source.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        if matches!(
            name.to_str(),
            Some(".git" | "build" | "dist" | "node_modules" | "target" | "out" | ".cache")
        ) {
            continue;
        }
        let destination = destination.join(name);
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &destination)?;
        } else {
            std::fs::copy(entry.path(), &destination).with_context(|| {
                format!(
                    "copying {} to {}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(destination)
        .with_context(|| format!("creating {}", destination.display()))?;
    for entry in
        std::fs::read_dir(source).with_context(|| format!("reading {}", source.display()))?
    {
        let entry = entry?;
        let destination = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &destination)?;
        } else {
            std::fs::copy(entry.path(), &destination).with_context(|| {
                format!(
                    "copying {} to {}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

    fn test_root() -> PathBuf {
        let suffix = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join(format!("fxpack-legacy-{suffix}"))
    }

    fn requested(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn copies_input_tree_into_isolated_scratch() {
        let root = test_root();
        let source = root.join("source");
        let scratch = root.join("scratch");
        std::fs::create_dir_all(source.join("generated")).unwrap();
        std::fs::write(
            source.join("fxdata.txt"),
            "include \"generated/value.txt\"\n",
        )
        .unwrap();
        std::fs::write(source.join("generated/value.txt"), "original").unwrap();

        copy_dir_recursive(&source, &scratch).unwrap();
        std::fs::write(scratch.join("generated/value.txt"), "scratch-only").unwrap();

        assert_eq!(
            std::fs::read_to_string(source.join("generated/value.txt")).unwrap(),
            "original"
        );
        assert_eq!(
            std::fs::read_to_string(scratch.join("generated/value.txt")).unwrap(),
            "scratch-only"
        );
    }

    #[test]
    fn parses_namespace_qualified_declaration_addresses() {
        let header = "constexpr uint24_t FX_DATA_BYTES = 12;\nnamespace EventData\n{\n  constexpr uint24_t first = 0x000002;\n  constexpr uint24_t second = 8;\n}\n";
        let output = extract_from_oracle_outputs(
            header,
            &[0; 12],
            &requested(&["EventData::first", "EventData::second"]),
        )
        .unwrap();
        assert_eq!(output.bytes("EventData::first").unwrap().len(), 6);
        assert_eq!(output.bytes("EventData::second").unwrap().len(), 4);
    }

    #[test]
    fn extracts_each_symbol_until_next_address_or_data_end() {
        let header = "constexpr uint24_t FX_DATA_BYTES = 8;\nconstexpr uint24_t alpha = 0x000000;\nconstexpr uint24_t beta = 0x000003;\n";
        let output = extract_from_oracle_outputs(
            header,
            &[0, 1, 2, 3, 4, 5, 6, 7],
            &requested(&["alpha", "beta"]),
        )
        .unwrap();
        assert_eq!(output.bytes("alpha").unwrap(), [0, 1, 2]);
        assert_eq!(output.bytes("beta").unwrap(), [3, 4, 5, 6, 7]);
    }

    #[test]
    fn missing_oracle_outputs_are_clear_errors() {
        let root = test_root();
        std::fs::create_dir_all(&root).unwrap();
        let error =
            read_requested_outputs(&root, &requested(&["EventData::eventTable"])).unwrap_err();
        assert!(format!("{error:#}").contains("oracle did not produce"));
        assert!(format!("{error:#}").contains("fxdata.h"));
    }

    #[test]
    fn extraction_output_is_deterministic() {
        let header = "constexpr uint24_t FX_DATA_BYTES = 6;\nnamespace Teams {\nconstexpr uint24_t teamList = 0x000003;\n}\nnamespace EventData {\nconstexpr uint24_t eventTable = 0x000000;\n}\n";
        let names = requested(&["Teams::teamList", "EventData::eventTable"]);
        let first = extract_from_oracle_outputs(header, &[1, 2, 3, 4, 5, 6], &names).unwrap();
        let second = extract_from_oracle_outputs(header, &[1, 2, 3, 4, 5, 6], &names).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            first.symbols().map(|(name, _)| name).collect::<Vec<_>>(),
            ["EventData::eventTable", "Teams::teamList"]
        );
    }

    #[test]
    #[ignore = "requires sibling CreatureGathererFX checkout, python3, and Pillow"]
    fn live_oracle_extracts_symbolic_event_and_team_declarations() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let tools_root = manifest_dir
            .ancestors()
            .nth(2)
            .expect("core crate has repository ancestors");
        let fx_root = tools_root
            .parent()
            .expect("tools repository has sibling directory")
            .join("CreatureGathererFX");
        let scratch_root = test_root();
        let fxdata_txt = fx_root.join("fxdata/fxdata.txt");
        let python_script = fx_root.join("Arduboy-Python-Utilities/fxdata-build.py");
        let inputs = LegacyBridgeInputs {
            fxdata_txt: &fxdata_txt,
            python_script: &python_script,
            python: Path::new("python3"),
            scratch_root: &scratch_root,
        };

        let output = extract(
            &inputs,
            &requested(&["EventData::eventTable", "Teams::teamList"]),
        )
        .unwrap();

        assert!(!output.bytes("EventData::eventTable").unwrap().is_empty());
        assert!(!output.bytes("Teams::teamList").unwrap().is_empty());
        assert!(!scratch_root.join("legacy-fxdata-build").exists());
    }
}
