//! Arena team-list builder.
//!
//! The legacy `tools/arena.py` reads `data/arena.csv` and writes a C/fxdata
//! `uint24_t teamList[]` declaration.  This module deliberately emits those
//! source bytes, rather than resolved FX addresses: address resolution belongs
//! to `fxdata-build.py` and changes with the surrounding image.

use crate::builder::{BuildError, verify_byte_parity};
use std::path::Path;
use std::process::Command;

/// One row of `data/arena.csv`.
///
/// These public fields are also the model values fixture emission needs: the
/// three stock-creature symbols, three levels, and trainer-name symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArenaEntry {
    pub first_creature: String,
    pub second_creature: String,
    pub third_creature: String,
    pub first_level: u8,
    pub second_level: u8,
    pub third_level: u8,
    pub trainer: String,
}

/// Parse the space-delimited `data/arena.csv` consumed by `tools/arena.py`.
pub fn parse_arena_csv(path: impl AsRef<Path>) -> Result<Vec<ArenaEntry>, BuildError> {
    let path = path.as_ref();
    let input = std::fs::read_to_string(path)
        .map_err(|e| BuildError(format!("reading {}: {e}", path.display())))?;
    let mut lines = input.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .ok_or_else(|| BuildError(format!("reading {}: missing header", path.display())))?;
    if header.split_whitespace().collect::<Vec<_>>() != ["c1", "c2", "c3", "l1", "l2", "l3", "name"]
    {
        return Err(BuildError(format!(
            "parsing {}: expected c1 c2 c3 l1 l2 l3 name header",
            path.display()
        )));
    }

    lines
        .enumerate()
        .map(|(index, line)| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() != 7 {
                return Err(BuildError(format!(
                    "parsing {} line {}: expected seven fields, got {}",
                    path.display(),
                    index + 2,
                    fields.len()
                )));
            }
            let level = |field: usize| {
                fields[field].parse::<u8>().map_err(|e| {
                    BuildError(format!(
                        "parsing {} line {} level {}: {e}",
                        path.display(),
                        index + 2,
                        field - 2
                    ))
                })
            };
            Ok(ArenaEntry {
                first_creature: fields[0].to_owned(),
                second_creature: fields[1].to_owned(),
                third_creature: fields[2].to_owned(),
                first_level: level(3)?,
                second_level: level(4)?,
                third_level: level(5)?,
                trainer: fields[6].to_owned(),
            })
        })
        .collect()
}

/// Build the exact UTF-8 payload printed by `python3 tools/arena.py`.
pub fn build_arena(entries: &[ArenaEntry]) -> Vec<u8> {
    let mut out = String::from("uint24_t teamList[] = {\n");
    for entry in entries {
        out.push_str(&format!(
            "\t{{{}, {}, {}, {}, {}, {}, {}}},\n",
            entry.first_creature,
            entry.second_creature,
            entry.third_creature,
            entry.first_level,
            entry.second_level,
            entry.third_level,
            entry.trainer,
        ));
    }
    out.push_str("}\n");
    out.into_bytes()
}

/// Compare a Rust payload with the reference generator through the shared
/// migration gate used by every data builder.
pub fn compare_arena_bytes(
    expected: &[u8],
    actual: &[u8],
    reference_command: &str,
    rust_command: &str,
) -> Result<(), BuildError> {
    verify_byte_parity(
        "arena.py stdout",
        expected,
        actual,
        reference_command,
        rust_command,
    )
}

/// Run the legacy generator on the same project data and byte-compare stdout.
pub fn verify_arena_parity(
    arena_csv: impl AsRef<Path>,
    arena_py: impl AsRef<Path>,
    rust_command: &str,
) -> Result<(), BuildError> {
    let arena_csv = arena_csv.as_ref();
    let arena_py = arena_py.as_ref();
    let entries = parse_arena_csv(arena_csv)?;
    let actual = build_arena(&entries);
    let working_dir = arena_csv
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| BuildError(format!("finding project root from {}", arena_csv.display())))?;
    let reference_command = format!("python3 {}", arena_py.display());
    let output = Command::new("python3")
        .arg(arena_py)
        .current_dir(working_dir)
        .output()
        .map_err(|e| BuildError(format!("running `{reference_command}`: {e}")))?;
    if !output.status.success() {
        return Err(BuildError(format!(
            "`{reference_command}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    compare_arena_bytes(&output.stdout, &actual, &reference_command, rust_command)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries() -> Vec<ArenaEntry> {
        vec![
            ArenaEntry {
                first_creature: "SquibbleSnail".into(),
                second_creature: "SkitterCrab".into(),
                third_creature: "squid".into(),
                first_level: 32,
                second_level: 32,
                third_level: 32,
                trainer: "jimbo".into(),
            },
            ArenaEntry {
                first_creature: "dragon".into(),
                second_creature: "skull".into(),
                third_creature: "ardu".into(),
                first_level: 1,
                second_level: 2,
                third_level: 3,
                trainer: "boss".into(),
            },
        ]
    }

    #[test]
    fn build_arena_matches_known_good_python_vector() {
        assert_eq!(
            build_arena(&entries()),
            b"uint24_t teamList[] = {\n\
\t{SquibbleSnail, SkitterCrab, squid, 32, 32, 32, jimbo},\n\
\t{dragon, skull, ardu, 1, 2, 3, boss},\n\
}\n"
        );
    }

    #[test]
    fn parse_arena_csv_exposes_fixture_model_fields() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/arena.csv");
        let parsed = parse_arena_csv(path).expect("arena fixture parses");
        assert_eq!(parsed, entries());
    }

    #[test]
    fn parity_reports_injected_one_byte_divergence() {
        let expected = b"abc";
        let actual = b"aXc";
        let err = compare_arena_bytes(
            expected,
            actual,
            "python3 tools/arena.py",
            "cgfx-tools --parity",
        )
        .expect_err("injected divergence must fail");
        assert_eq!(
            err.0,
            "arena.py stdout parity mismatch at byte 1: expected 0x62 from `python3 tools/arena.py`, got 0x58 from `cgfx-tools --parity`"
        );
    }
}
