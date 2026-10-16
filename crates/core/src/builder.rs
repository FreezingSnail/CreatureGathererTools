//! Game-data builders, ported from creatureTools internal/builder
//! (creatures.go, moves.go). Emits the same `.bin` / `.hpp` artifacts
//! byte-for-byte as the Go builders; Go output is the reference contract.
//!
//! - `creatureData.bin`  : 14 bytes per creature, little-endian-ish raw
//!   bytes (each field truncated to one byte, in model order).
//! - `creatureData.hpp`  : `namespace CreatureData { uint8_t creatureData[] = {...}; }`
//! - `moves.bin`         : 4 bytes per move: packed u16 (big-endian) +
//!   effect byte + 0xFF effect2 byte.
//! - `moves.hpp`         : per-move name strings + `moveNames` pointer table.
//! - `moves.txt`         : byte-identical copy of `moves.hpp`, consumed by the
//!   FX layout `expand` entry as the MoveData name declarations.

use crate::config::Config;
use crate::model::{Creature, GameMap, Move};
use crate::script;
use crate::script::ToBytecode;
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Builder failure with a human-readable message (mirrors Go's `error`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildError(pub String);

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BuildError {}

fn io_err(action: &str, path: &Path, e: std::io::Error) -> BuildError {
    BuildError(format!("{action} {}: {e}", path.display()))
}

/// Go `creatures.go` `creatureTypeIndex`: type name -> byte.
const CREATURE_TYPE_INDEX: [(&str, u8); 9] = [
    ("spirit", 0),
    ("water", 1),
    ("wind", 2),
    ("earth", 3),
    ("fire", 4),
    ("lightning", 5),
    ("plant", 6),
    ("elder", 7),
    ("none", 8),
];

fn creature_type_byte(ty: &str) -> Option<u8> {
    CREATURE_TYPE_INDEX
        .iter()
        .find(|(name, _)| *name == ty)
        .map(|(_, b)| *b)
}

/// Go `moves.go` `moveTypeIndex`: type name -> u16.
const MOVE_TYPE_INDEX: [(&str, u16); 9] = [
    ("spirit", 0),
    ("water", 1),
    ("wind", 2),
    ("earth", 3),
    ("fire", 4),
    ("lightning", 5),
    ("plant", 6),
    ("elder", 7),
    ("status", 8),
    // Go also maps "none" -> 15.
];

const MOVE_TYPE_NONE: u16 = 15;

fn move_type_index(ty: &str) -> Option<u16> {
    MOVE_TYPE_INDEX
        .iter()
        .find(|(name, _)| *name == ty)
        .map(|(_, v)| *v)
        .or_else(|| (ty == "none").then_some(MOVE_TYPE_NONE))
}

/// One type-effectiveness cell. Values match `type_table_data.py` exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeEffectiveness {
    None,
    Half,
    Same,
    Double,
}

impl TypeEffectiveness {
    fn byte(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Half => 2,
            Self::Same => 3,
            Self::Double => 4,
        }
    }

    fn parse(value: &str) -> Result<Self, BuildError> {
        match value.trim().parse::<f32>() {
            Ok(0.0) => Ok(Self::None),
            Ok(0.5) => Ok(Self::Half),
            Ok(1.0) => Ok(Self::Same),
            Ok(2.0) => Ok(Self::Double),
            Ok(_) | Err(_) => Err(BuildError(format!(
                "unknown type effectiveness modifier {value:?}; expected 0, 0.5, 1, or 2"
            ))),
        }
    }
}

/// A named row in `data/typetable.csv`. The name is retained so repeated names
/// replace their prior row, as assignment to Python's `dict` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeTableRow {
    pub name: String,
    pub modifiers: Vec<TypeEffectiveness>,
}

/// Parsed source model for the legacy tab-separated type table.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypeTable {
    pub rows: Vec<TypeTableRow>,
}

/// Loads `data/typetable.csv`, matching `type_table_data.py`: skip the header,
/// parse tab-separated modifiers, and preserve the Python dictionary row order.
pub fn load_type_table(path: impl AsRef<Path>) -> Result<TypeTable, BuildError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path).map_err(|e| io_err("reading", path, e))?;
    let mut lines = source.lines();
    lines
        .next()
        .ok_or_else(|| BuildError(format!("reading {}: missing header", path.display())))?;

    let mut table = TypeTable::default();
    for (line_number, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let name = fields
            .next()
            .filter(|name| !name.is_empty())
            .ok_or_else(|| {
                BuildError(format!(
                    "reading {}: empty type at line {}",
                    path.display(),
                    line_number + 2
                ))
            })?;
        let modifiers = fields
            .map(TypeEffectiveness::parse)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                BuildError(format!(
                    "reading {} line {}: {e}",
                    path.display(),
                    line_number + 2
                ))
            })?;
        let row = TypeTableRow {
            name: name.to_owned(),
            modifiers,
        };
        if let Some(index) = table.rows.iter().position(|old| old.name == row.name) {
            table.rows[index] = row;
        } else {
            table.rows.push(row);
        }
    }
    Ok(table)
}

/// Builds `typetable.bin` in the byte order emitted by
/// `tools/data_converters/type_table_data.py`.
pub fn build_type_table(table: &TypeTable) -> Vec<u8> {
    table
        .rows
        .iter()
        .flat_map(|row| row.modifiers.iter().copied().map(TypeEffectiveness::byte))
        .collect()
}

/// Writes `typetable.bin` into an artifact directory.
pub fn write_type_table(output_dir: impl AsRef<Path>, table: &TypeTable) -> Result<(), BuildError> {
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir).map_err(|e| io_err("creating", output_dir, e))?;
    write_bytes(&output_dir.join("typetable.bin"), &build_type_table(table))
}

const CONSUMABLE_COUNT: usize = 8;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsumableKind {
    None = 0,
    Heal = 1,
    Cure = 2,
    Charge = 3,
}

impl ConsumableKind {
    fn parse(value: &str) -> Result<Self, BuildError> {
        match value {
            "None" => Ok(Self::None),
            "Heal" => Ok(Self::Heal),
            "Cure" => Ok(Self::Cure),
            "Charge" => Ok(Self::Charge),
            _ => Err(BuildError(format!(
                "unknown consumable kind {value:?}; expected None, Heal, Cure, or Charge"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsumableDef {
    pub kind: ConsumableKind,
    pub arg: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumableTable {
    pub rows: [ConsumableDef; CONSUMABLE_COUNT],
}

fn parse_consumables_csv_source(
    source: &str,
    source_name: &str,
) -> Result<ConsumableTable, BuildError> {
    let mut header_seen = false;
    let mut row_count = 0usize;
    let empty = ConsumableDef {
        kind: ConsumableKind::None,
        arg: 0,
    };
    let mut rows = [empty; CONSUMABLE_COUNT];
    let mut seen = [false; CONSUMABLE_COUNT];

    for (line_index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !header_seen {
            header_seen = true;
            if line != "id,kind,arg" {
                return Err(BuildError(format!(
                    "reading {source_name} line {}: expected header id,kind,arg",
                    line_index + 1
                )));
            }
            continue;
        }

        row_count += 1;
        let fields: Vec<_> = line.split(',').map(str::trim).collect();
        if fields.len() != 3 {
            return Err(BuildError(format!(
                "reading {source_name} line {}: expected 3 columns id,kind,arg",
                line_index + 1
            )));
        }
        let id = fields[0].parse::<usize>().map_err(|_| {
            BuildError(format!(
                "reading {source_name} line {}: invalid consumable id {:?}",
                line_index + 1,
                fields[0]
            ))
        })?;
        if id >= CONSUMABLE_COUNT {
            return Err(BuildError(format!(
                "reading {source_name} line {}: consumable id {id} is outside 0..{}",
                line_index + 1,
                CONSUMABLE_COUNT - 1
            )));
        }
        if seen[id] {
            return Err(BuildError(format!(
                "reading {source_name} line {}: duplicate consumable id {id}",
                line_index + 1
            )));
        }
        let kind = ConsumableKind::parse(fields[1]).map_err(|error| {
            BuildError(format!(
                "reading {source_name} line {}: {error}",
                line_index + 1
            ))
        })?;
        let arg = fields[2].parse::<u8>().map_err(|_| {
            BuildError(format!(
                "reading {source_name} line {}: invalid consumable arg {:?}; expected 0..255",
                line_index + 1,
                fields[2]
            ))
        })?;
        rows[id] = ConsumableDef { kind, arg };
        seen[id] = true;
    }

    if !header_seen {
        return Err(BuildError(format!("reading {source_name}: missing header")));
    }
    if row_count != CONSUMABLE_COUNT {
        return Err(BuildError(format!(
            "reading {source_name}: expected {CONSUMABLE_COUNT} rows, found {row_count}"
        )));
    }
    for (id, present) in seen.iter().enumerate() {
        if !present {
            return Err(BuildError(format!(
                "reading {source_name}: missing consumable id {id}"
            )));
        }
    }
    Ok(ConsumableTable { rows })
}

/// Loads exactly eight indexed consumables from the canonical CSV source.
pub fn load_consumables_csv(path: impl AsRef<Path>) -> Result<ConsumableTable, BuildError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path).map_err(|e| io_err("reading", path, e))?;
    parse_consumables_csv_source(&source, &path.display().to_string())
}

/// Builds eight two-byte records in id order: kind followed by arg.
pub fn build_consumables(table: &ConsumableTable) -> Vec<u8> {
    table
        .rows
        .iter()
        .flat_map(|row| [row.kind as u8, row.arg])
        .collect()
}

/// Writes `consumables.bin` into an artifact directory.
pub fn write_consumables(
    output_dir: impl AsRef<Path>,
    table: &ConsumableTable,
) -> Result<(), BuildError> {
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir).map_err(|e| io_err("creating", output_dir, e))?;
    write_bytes(
        &output_dir.join("consumables.bin"),
        &build_consumables(table),
    )
}

/// Verifies a generated artifact against its reference bytes. Shared by data
/// builders so every parity failure identifies the first divergent byte and
/// both commands needed to reproduce it.
pub fn verify_byte_parity(
    artifact: &str,
    expected: &[u8],
    actual: &[u8],
    expected_command: &str,
    actual_command: &str,
) -> Result<(), BuildError> {
    if expected == actual {
        return Ok(());
    }

    let offset = (0..expected.len().max(actual.len()))
        .find(|&index| expected.get(index) != actual.get(index))
        .expect("different byte slices have a divergent offset");
    let display_byte = |byte: Option<&u8>| match byte {
        Some(byte) => format!("0x{byte:02X}"),
        None => "<EOF>".to_owned(),
    };
    Err(BuildError(format!(
        "{artifact} parity mismatch at byte {offset}: expected {} from `{expected_command}`, got {} from `{actual_command}`",
        display_byte(expected.get(offset)),
        display_byte(actual.get(offset)),
    )))
}

/// Runs `type_table_data.py --format fx` from the source-data project then
/// checks its generated `typetable.bin`. `csv` must be
/// `<project>/data/typetable.csv` so both implementations consume one source.
pub fn verify_type_table_python_parity(
    csv: impl AsRef<Path>,
    rust_bytes: &[u8],
    rust_command: &str,
) -> Result<(), BuildError> {
    let csv = csv.as_ref();
    let root = csv.parent().and_then(Path::parent).ok_or_else(|| {
        BuildError(format!(
            "{}: expected <project>/data/typetable.csv",
            csv.display()
        ))
    })?;
    let python_command = "python3 tools/data_converters/type_table_data.py --format fx";
    let status = std::process::Command::new("python3")
        .args(["tools/data_converters/type_table_data.py", "--format", "fx"])
        .current_dir(root)
        .status()
        .map_err(|err| BuildError(format!("running {python_command}: {err}")))?;
    if !status.success() {
        return Err(BuildError(format!(
            "reference converter failed ({status}): cd {} && {python_command}",
            root.display()
        )));
    }
    let output = root.join("fxdata/generated/typetable.bin");
    let python_bytes = std::fs::read(&output).map_err(|err| io_err("reading", &output, err))?;
    verify_byte_parity(
        "typetable.bin",
        &python_bytes,
        rust_bytes,
        &format!("cd {} && {python_command}", root.display()),
        rust_command,
    )
}

/// Go `moves.go` `effectIndex`: effect name -> byte (NONE -> 0xFF).
const EFFECT_INDEX: [(&str, u8); 31] = [
    ("NONE", 0xFF),
    ("DPRSD", 0),
    ("SOAKED", 1),
    ("BUFTD", 2),
    ("SOILED", 3),
    ("SCRCHD", 4),
    ("ZAPPED", 5),
    ("TANGLD", 6),
    ("REDCD", 7),
    ("ENLTND", 8),
    ("DRNCHD", 9),
    ("AIRSWPT", 10),
    ("GRNDED", 11),
    ("KINDLD", 12),
    ("CHRGD", 13),
    ("ENRCHD", 14),
    ("EVOLVD", 15),
    ("ATKDWN", 16),
    ("DEFDWN", 17),
    ("SPCADWN", 18),
    ("SPCDDWN", 19),
    ("SPDDWN", 20),
    ("ATKUP", 21),
    ("DEFUP", 22),
    ("SPCAUP", 23),
    ("SPCDUP", 24),
    ("SPDUP", 25),
    ("SAPPD", 26),
    ("INFSED", 27),
    ("PINNED", 28),
    ("CONCUSED", 29),
];

fn effect_index(effect: &str) -> Option<u8> {
    EFFECT_INDEX
        .iter()
        .find(|(name, _)| *name == effect)
        .map(|(_, v)| *v)
}

/// Go `moves.go` `accuracyIndex`: accuracy -> 2-bit index (unknown -> 0).
fn accuracy_index(accuracy: i32) -> u16 {
    match accuracy {
        100 => 0,
        90 => 1,
        80 => 2,
        70 => 3,
        _ => 0,
    }
}

/// Authoring HP bound for the one-byte per-instance HP encoding (jp8.11.22):
/// the firmware derives max HP as `2*level + hpSeed*(level/3) + 30` truncated
/// to a byte, and stores HP as `0 = legacy/full`, `1..=255 = actualHP + 1`.
/// Both need `maxHP <= 254` for every supported level, and the seed itself must
/// fit the one-byte `CreatureData_t.hpSeed` field.
fn validate_creature_hp(creature: &Creature) -> Result<(), BuildError> {
    if creature.hp < 0 || creature.hp > 255 {
        return Err(BuildError(format!(
            "creature {} HP seed {} is outside the 0..=255 byte range",
            creature.name, creature.hp
        )));
    }
    for level in 1u8..=31 {
        let max_hp = 2 * level as i64 + creature.hp as i64 * (level as i64 / 3) + 30;
        if max_hp > 254 {
            return Err(BuildError(format!(
                "creature {} HP seed {} yields max HP {} at level {} above 254",
                creature.name, creature.hp, max_hp, level
            )));
        }
    }
    Ok(())
}

/// Mirrors Go `BuildCreatures`: produces the `creatureData.bin` bytes and
/// the `creatureData.hpp` text for the given creatures.
pub fn build_creatures(creatures: &[Creature]) -> Result<(Vec<u8>, String), BuildError> {
    let mut bin = Vec::with_capacity(creatures.len() * 14);
    let mut table = String::new();
    table.push_str("namespace CreatureData {\n\n    uint8_t creatureData[] = {\n");
    for c in creatures {
        validate_creature_hp(c)?;
        let t1 = creature_type_byte(&c.type1)
            .ok_or_else(|| BuildError(format!("unknown type for creature {}", c.name)))?;
        let t2 = creature_type_byte(&c.type2)
            .ok_or_else(|| BuildError(format!("unknown type for creature {}", c.name)))?;
        bin.extend_from_slice(&[
            c.id as u8,
            t1,
            t2,
            c.evo_level as u8,
            c.atk as u8,
            c.def as u8,
            c.spc_atk as u8,
            c.spc_def as u8,
            c.hp as u8,
            c.spd as u8,
            c.move1 as u8,
            c.move2 as u8,
            c.move3 as u8,
            c.move4 as u8,
        ]);
        table.push_str(&format!(
            "    {{{}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}}},\n",
            c.id,
            t1,
            t2,
            c.evo_level,
            c.atk,
            c.def,
            c.spc_atk,
            c.spc_def,
            c.hp,
            c.spd,
            c.move1,
            c.move2,
            c.move3,
            c.move4
        ));
    }
    table.push_str("    };\n\n}\n");
    Ok((bin, table))
}

/// Mirrors Go `BuildMoves`: produces the `moves.bin` bytes and the
/// `moves.hpp` text for the given moves.
pub fn build_moves(moves: &[Move]) -> Result<(Vec<u8>, String), BuildError> {
    let mut bin = Vec::with_capacity(moves.len() * 4);
    let mut hpp = String::from("namespace MoveData {\n");
    for m in moves {
        let t = move_type_index(&m.r#type).ok_or_else(|| {
            BuildError(format!(
                "unknown move type {:?} for move {}",
                m.r#type, m.name
            ))
        })?;
        let acc = accuracy_index(m.accuracy);
        let packed =
            ((t as u16) << 12) | ((m.power as u16) << 7) | ((m.physical as u16) << 6) | (acc << 4);
        let eff = effect_index(&m.effect).unwrap_or(0xFF);
        bin.extend_from_slice(&packed.to_be_bytes());
        bin.push(eff);
        bin.push(0xFF); // effect2 always NONE
        hpp.push_str(&format!(
            "    uint8_t move{}[] = \"{}\\0\";\n",
            m.id, m.name
        ));
    }
    hpp.push_str("\n    uint24_t moveNames[] = {\n        ");
    for (i, m) in moves.iter().enumerate() {
        if i > 0 {
            hpp.push_str(", ");
        }
        hpp.push_str(&format!("move{}", m.id));
    }
    hpp.push_str("\n    };\n\n}\n");
    Ok((bin, hpp))
}

/// Mirrors Go `BuildCreatures` file side: writes `creatureData.bin` and
/// `creatureData.hpp` into `cfg.output_dir` (creating it if needed).
pub fn write_creatures(cfg: &Config, creatures: &[Creature]) -> Result<(), BuildError> {
    let (bin, hpp) = build_creatures(creatures)?;
    let dir = Path::new(&cfg.output_dir);
    std::fs::create_dir_all(dir).map_err(|e| io_err("creating", dir, e))?;
    let bin_path = dir.join("creatureData.bin");
    let hpp_path = dir.join("creatureData.hpp");
    std::fs::write(&bin_path, &bin).map_err(|e| io_err("writing", &bin_path, e))?;
    std::fs::write(&hpp_path, hpp).map_err(|e| io_err("writing", &hpp_path, e))?;
    Ok(())
}

/// Mirrors Go `BuildMoves` file side: writes `moves.bin`, `moves.hpp` and the
/// `moves.txt` FX name-declaration input into `cfg.output_dir` (creating it if
/// needed).
///
/// `moves.txt` carries exactly the same bytes as `moves.hpp`: the layout's
/// `expand` entry consumes the name declarations and the `moveNames` pointer
/// table that the header reproduces. Emitting it here keeps a fresh tracked
/// checkout able to generate the MoveData input instead of relying on a stale
/// ignored artifact.
pub fn write_moves(cfg: &Config, moves: &[Move]) -> Result<(), BuildError> {
    let (bin, hpp) = build_moves(moves)?;
    let dir = Path::new(&cfg.output_dir);
    std::fs::create_dir_all(dir).map_err(|e| io_err("creating", dir, e))?;
    let bin_path = dir.join("moves.bin");
    let hpp_path = dir.join("moves.hpp");
    let txt_path = dir.join("moves.txt");
    std::fs::write(&bin_path, &bin).map_err(|e| io_err("writing", &bin_path, e))?;
    std::fs::write(&hpp_path, &hpp).map_err(|e| io_err("writing", &hpp_path, e))?;
    std::fs::write(&txt_path, hpp).map_err(|e| io_err("writing", &txt_path, e))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// FX text descriptors consumed by fxdata-build.py. These use the FX type
// encoding (`none = 15`), which intentionally differs from the legacy Go
// creature binary's `none = 8` encoding above.

fn fx_type_byte(ty: &str) -> Option<u8> {
    match ty {
        "spirit" => Some(0),
        "water" => Some(1),
        "wind" => Some(2),
        "earth" => Some(3),
        "fire" => Some(4),
        "lightning" => Some(5),
        "plant" => Some(6),
        "elder" => Some(7),
        "status" => Some(8),
        "none" => Some(15),
        _ => None,
    }
}

/// Builds the `creatureData.txt` descriptor emitted by
/// `tools/data_converters/creature_data.py`.
pub fn build_fx_creature_descriptor(creatures: &[Creature]) -> Result<String, BuildError> {
    let mut output = String::from(
        "// Generated from data/Creaturesheet.csv by tools/data_converters/creature_data.py.\n\
         // Do not edit: regenerate with python3 tools/data_converters/creature_data.py.\n\
         namespace CreatureData {\n\nuint8_t creatureData[] = {\n",
    );
    for creature in creatures {
        let type1 = fx_type_byte(&creature.type1)
            .ok_or_else(|| BuildError(format!("unknown FX type for creature {}", creature.name)))?;
        let type2 = fx_type_byte(&creature.type2)
            .ok_or_else(|| BuildError(format!("unknown FX type for creature {}", creature.name)))?;
        output.push_str(&format!(
            "    {{{}, {type1}, {type2}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}}},\n",
            creature.id,
            creature.evo_level,
            creature.atk,
            creature.def,
            creature.spc_atk,
            creature.spc_def,
            creature.hp,
            creature.spd,
            creature.move1,
            creature.move2,
            creature.move3,
            creature.move4,
        ));
    }
    output.push_str("};\n}\nnamespace_end\n");
    Ok(output)
}

/// Builds `movedata.txt`, the legacy FX move-table descriptor. `15` is the
/// fixed secondary-type sentinel used by the original generated descriptor.
pub fn build_fx_move_descriptor(moves: &[Move]) -> Result<String, BuildError> {
    let mut output = String::from("namespace MoveData {\nuint8_t movePack[] = {\n");
    let mut ordered: Vec<&Move> = moves.iter().collect();
    ordered.sort_by_key(|movement| movement.id != 255);
    for movement in ordered {
        let ty = fx_type_byte(&movement.r#type).ok_or_else(|| {
            BuildError(format!(
                "unknown FX move type {:?} for move {}",
                movement.r#type, movement.name
            ))
        })?;
        let effect = effect_index(&movement.effect).unwrap_or(0xFF);
        output.push_str(&format!(
            "{{ {ty}, {}, 15, {}, {effect} }},\n",
            movement.power, movement.accuracy
        ));
    }
    output.push_str("};\n} namespace_end\n");
    Ok(output)
}

/// Builds `opponents.txt`, preserving the per-opponent labels and 6-byte row
/// grouping emitted by `opponent_data.py --format fx`.
pub fn build_fx_opponents_descriptor(opponents: &[OpponentSeed]) -> String {
    let mut output = String::new();
    for (index, opponent) in opponents.iter().enumerate() {
        let label = if index == 0 {
            "opponent_seeds".to_owned()
        } else {
            format!("opponent_seed_{index}")
        };
        output.push_str(&format!("uint8_t {label}[] = {{\n"));
        let bytes = build_opponents(std::slice::from_ref(opponent));
        for row in bytes.chunks_exact(6) {
            let values = row
                .iter()
                .map(|byte| format!("0x{byte:02X}"))
                .collect::<Vec<_>>()
                .join(", ");
            output.push_str(&format!("    {values},\n"));
        }
        output.push_str("};\n\n");
    }
    output.push_str("uint24_t opts[] = {\n");
    for index in 0..opponents.len() {
        let label = if index == 0 {
            "opponent_seeds".to_owned()
        } else {
            format!("opponent_seed_{index}")
        };
        output.push_str(&format!("    {label},\n"));
    }
    output.push_str("};\n");
    output
}

/// Writes all FX text descriptors into one artifact directory.
pub fn write_fx_descriptors(
    output_dir: impl AsRef<Path>,
    creatures: &[Creature],
    moves: &[Move],
    opponents: &[OpponentSeed],
) -> Result<(), BuildError> {
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir).map_err(|e| io_err("creating", output_dir, e))?;
    let creature_path = output_dir.join("creatureData.txt");
    let move_path = output_dir.join("movedata.txt");
    let opponent_path = output_dir.join("opponents.txt");
    write_str(&creature_path, &build_fx_creature_descriptor(creatures)?)?;
    write_str(&move_path, &build_fx_move_descriptor(moves)?)?;
    write_str(&opponent_path, &build_fx_opponents_descriptor(opponents))
}

/// Captured Python-reference descriptor bytes for a shared project parity run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FxDescriptorReference {
    pub opponents: Vec<u8>,
    pub creatures: Vec<u8>,
    pub moves: Vec<u8>,
}

/// Runs the available Python descriptor writers before Rust output overwrites
/// the same FX paths, then captures all three reference descriptors.
pub fn capture_project_fx_descriptor_reference(
    project_root: impl AsRef<Path>,
) -> Result<FxDescriptorReference, BuildError> {
    let root = project_root.as_ref();
    for script in [
        "tools/data_converters/opponent_data.py",
        "tools/data_converters/creature_data.py",
    ] {
        let status = Command::new("python3")
            .arg(script)
            .current_dir(root)
            .status()
            .map_err(|e| BuildError(format!("running python3 {script}: {e}")))?;
        if !status.success() {
            return Err(BuildError(format!(
                "reference converter failed ({status}): cd {} && python3 {script}",
                root.display()
            )));
        }
    }
    let read = |relative: &str| {
        let path = root.join(relative);
        std::fs::read(&path).map_err(|e| io_err("reading", &path, e))
    };
    Ok(FxDescriptorReference {
        opponents: read("fxdata/generated/opponents.txt")?,
        creatures: read("fxdata/data/creatureData.txt")?,
        moves: read("fxdata/generated/movedata.txt")?,
    })
}

/// Checks all FX descriptors through the shared byte-parity diagnostic.
pub fn verify_fx_descriptor_parity(
    reference: &FxDescriptorReference,
    actual: &FxDescriptorReference,
    python_command: &str,
    rust_command: &str,
) -> Result<(), BuildError> {
    for (artifact, expected, got) in [
        ("opponents.txt", &reference.opponents, &actual.opponents),
        ("creatureData.txt", &reference.creatures, &actual.creatures),
        ("movedata.txt", &reference.moves, &actual.moves),
    ] {
        verify_byte_parity(artifact, expected, got, python_command, rust_command)?;
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Opponent builder. The canonical source remains `data/opponents.csv`: it is
// deliberately not moved to JSON because `tools/data_converters/opponent_data.py`
// reads that same CSV. Keeping one source makes Python/Rust byte parity a direct
// migration gate.
//
// Each `OpponentSeed` is three `<BBI` records (creature ID, level, packed moves)
// and therefore exactly 18 bytes, little-endian, matching Python `struct.pack`.
// ─────────────────────────────────────────────────────────────────────

use crate::model::{OpponentPartyMember, OpponentSeed};
use std::process::Command;

const OPPONENT_CSV_COLUMNS: usize = 19;

fn creature_id(name: &str) -> Option<u8> {
    Some(match name {
        "none" => 255,
        "squibblesnail" => 0,
        "squablesnail" => 1,
        "scramblesnail" => 2,
        "skittercrab" => 3,
        "scattercrab" => 4,
        "shattercrab" => 5,
        "squid" => 6,
        "bigsquid" => 7,
        "biggestsquid" => 8,
        "bell" => 9,
        "ember" => 10,
        "cuircuit" => 11,
        "hedge" => 12,
        "cloud" => 13,
        "rock" => 14,
        "wiggleworm" => 15,
        "waggleworm" => 16,
        "skimskate" => 17,
        "skimray" => 18,
        "billow" => 19,
        "howl" => 20,
        "item" => 21,
        "item2" => 22,
        "zip" => 23,
        "zap" => 24,
        "suculent" => 25,
        "cactus" => 26,
        "flickerfly" => 27,
        "flitfly" => 28,
        "dragon" => 29,
        "skull" => 30,
        "ardu" => 31,
        _ => return None,
    })
}

fn move_id(name: &str) -> Option<u8> {
    Some(match name {
        "none" => 255,
        "smite" => 0,
        "thought" => 1,
        "wisper" => 2,
        "blast" => 3,
        "splash" => 4,
        "squirt" => 5,
        "soak" => 6,
        "current" => 7,
        "breeze" => 8,
        "bellow" => 9,
        "torrent" => 10,
        "blow" => 11,
        "dirtburst" => 12,
        "slipfall" => 13,
        "dirtfall" => 14,
        "shatter" => 15,
        "burn" => 16,
        "melt" => 17,
        "torch" => 18,
        "stoke" => 19,
        "bolt" => 20,
        "plasma" => 21,
        "coil" => 22,
        "zap" => 23,
        "pollen" => 24,
        "pollenburst" => 25,
        "seedfall" => 26,
        "root" => 27,
        "burst" => 28,
        "elderBurst" => 29,
        "slam" => 30,
        "elderSlam" => 31,
        _ => return None,
    })
}

fn parse_opponent_member(
    row: &[&str],
    id_col: usize,
    level_col: usize,
    moves_col: usize,
    line: usize,
) -> Result<OpponentPartyMember, BuildError> {
    let creature_id = creature_id(row[id_col])
        .ok_or_else(|| BuildError(format!("line {line}: unknown creature {:?}", row[id_col])))?;
    let level = row[level_col].parse::<u8>().map_err(|err| {
        BuildError(format!(
            "line {line}: invalid level {:?}: {err}",
            row[level_col]
        ))
    })?;
    let mut moves = 0u32;
    for offset in 0..4 {
        let move_id = move_id(row[moves_col + offset]).ok_or_else(|| {
            BuildError(format!(
                "line {line}: unknown move {:?}",
                row[moves_col + offset]
            ))
        })?;
        moves |= u32::from(move_id) << (offset * 8);
    }
    Ok(OpponentPartyMember {
        creature_id,
        level,
        moves,
    })
}

/// Loads the canonical Python-compatible `data/opponents.csv` schema. The
/// name column is metadata; binary output contains only the three party slots.
pub fn load_opponents_csv(path: impl AsRef<Path>) -> Result<Vec<OpponentSeed>, BuildError> {
    load_opponents_csv_with(path.as_ref(), |row, _member, id, level, moves, line| {
        parse_opponent_member(row, id, level, moves, line)
    })
}

fn load_opponents_csv_with<F>(
    path: &Path,
    mut parse_member: F,
) -> Result<Vec<OpponentSeed>, BuildError>
where
    F: FnMut(&[&str], &str, usize, usize, usize, usize) -> Result<OpponentPartyMember, BuildError>,
{
    let text = std::fs::read_to_string(path).map_err(|e| io_err("reading", path, e))?;
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Err(BuildError(format!(
            "{}: missing CSV header",
            path.display()
        )));
    };
    if header.split(',').count() != OPPONENT_CSV_COLUMNS {
        return Err(BuildError(format!(
            "{}: expected {OPPONENT_CSV_COLUMNS} columns in CSV header",
            path.display()
        )));
    }
    lines
        .enumerate()
        .map(|(index, line)| {
            let line_number = index + 2;
            let row: Vec<_> = line.split(',').collect();
            if row.len() != OPPONENT_CSV_COLUMNS {
                return Err(BuildError(format!(
                    "{}:{line_number}: expected {OPPONENT_CSV_COLUMNS} columns, found {}",
                    path.display(),
                    row.len()
                )));
            }
            Ok(OpponentSeed {
                first: parse_member(&row, "first", 0, 3, 7, line_number)?,
                second: parse_member(&row, "second", 1, 4, 11, line_number)?,
                third: parse_member(&row, "third", 2, 5, 15, line_number)?,
            })
        })
        .collect()
}

/// Loads project opponent rows using the canonical move table. Names are exact
/// case matches; `none` remains 255 and move slots are packed little-endian.
pub fn load_opponents_csv_with_moves(
    path: impl AsRef<Path>,
    moves: &[Move],
) -> Result<Vec<OpponentSeed>, BuildError> {
    let path = path.as_ref();
    let mut ids = HashMap::new();
    let mut ambiguous = std::collections::HashSet::new();
    for movement in moves {
        // `none` has a CSV sentinel encoding (255), independent of its
        // placeholder row in moves.json.
        if movement.name == "none" {
            continue;
        }
        let id = u8::try_from(movement.id)
            .ok()
            .filter(|id| *id != 255)
            .ok_or_else(|| {
                BuildError(format!(
                    "{}: invalid canonical move ID {} for {:?}",
                    path.display(),
                    movement.id,
                    movement.name
                ))
            })?;
        if ids.insert(movement.name.as_str(), id).is_some() {
            ambiguous.insert(movement.name.as_str());
        }
    }
    load_opponents_csv_with(path, |row, member, id_col, level_col, moves_col, line| {
        let context = |message: String| {
            BuildError(format!(
                "{}:{line}: {member} party member: {message}",
                path.display()
            ))
        };
        let creature_id = creature_id(row[id_col])
            .ok_or_else(|| context(format!("unknown creature {:?}", row[id_col])))?;
        let level = row[level_col]
            .parse::<u8>()
            .map_err(|err| context(format!("invalid level {:?}: {err}", row[level_col])))?;
        let mut packed = 0u32;
        for offset in 0..4 {
            let name = row[moves_col + offset];
            let move_id = if name == "none" {
                255
            } else if ambiguous.contains(name) {
                return Err(context(format!(
                    "moves[{}] has ambiguous move name {name:?}",
                    offset + 1
                )));
            } else {
                *ids.get(name).ok_or_else(|| {
                    context(format!("moves[{}] has unknown move {name:?}", offset + 1))
                })?
            };
            packed |= u32::from(move_id) << (offset * 8);
        }
        Ok(OpponentPartyMember {
            creature_id,
            level,
            moves: packed,
        })
    })
}

/// Produces `opponents.bin`: three little-endian `<BBI` records per opponent.
pub fn build_opponents(opponents: &[OpponentSeed]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(opponents.len() * 18);
    for opponent in opponents {
        for member in [opponent.first, opponent.second, opponent.third] {
            bytes.push(member.creature_id);
            bytes.push(member.level);
            bytes.extend_from_slice(&member.moves.to_le_bytes());
        }
    }
    bytes
}

fn parity_byte(byte: Option<u8>) -> String {
    byte.map_or_else(|| "<end>".to_string(), |byte| format!("0x{byte:02X}"))
}

/// Compares Python (expected) and Rust (actual) output. Diagnostics are
/// command-ready so a byte mismatch can be reproduced directly.
pub fn compare_opponent_bytes(
    python_bytes: &[u8],
    rust_bytes: &[u8],
    python_command: &str,
    rust_command: &str,
) -> Result<(), BuildError> {
    let differing_offset = (0..python_bytes.len().max(rust_bytes.len()))
        .find(|&offset| python_bytes.get(offset) != rust_bytes.get(offset));
    match differing_offset {
        None => Ok(()),
        Some(offset) => Err(BuildError(format!(
            "opponents parity mismatch at offset {offset}: expected {}, actual {}\npython: {python_command}\nrust: {rust_command}",
            parity_byte(python_bytes.get(offset).copied()),
            parity_byte(rust_bytes.get(offset).copied()),
        ))),
    }
}

/// Runs `opponent_data.py --format fx` from the source-data project then checks
/// its `fxdata/generated/opponents.bin`. `csv` must be `<project>/data/opponents.csv`.
pub fn verify_opponents_python_parity(
    csv: impl AsRef<Path>,
    rust_bytes: &[u8],
    rust_command: &str,
) -> Result<(), BuildError> {
    let csv = csv.as_ref();
    let root = csv.parent().and_then(Path::parent).ok_or_else(|| {
        BuildError(format!(
            "{}: expected <project>/data/opponents.csv",
            csv.display()
        ))
    })?;
    let python_command = "python3 tools/data_converters/opponent_data.py --format fx";
    let status = Command::new("python3")
        .args(["tools/data_converters/opponent_data.py", "--format", "fx"])
        .current_dir(root)
        .status()
        .map_err(|err| BuildError(format!("running {python_command}: {err}")))?;
    if !status.success() {
        return Err(BuildError(format!(
            "reference converter failed ({status}): cd {} && {python_command}",
            root.display()
        )));
    }
    let output = root.join("fxdata/generated/opponents.bin");
    let python_bytes = std::fs::read(&output).map_err(|err| io_err("reading", &output, err))?;
    compare_opponent_bytes(
        &python_bytes,
        rust_bytes,
        &format!("cd {} && {python_command}", root.display()),
        rust_command,
    )
}

// ─────────────────────────────────────────────────────────────────────
// Map builder, ported from creatureTools internal/builder/mapbuilder.go.
// Go output is the byte-for-byte reference contract:
//
// - `raw_map.bin` : 256×256 encoded tile+property words, u16 LE row-major,
//   out-of-range → 0.
// - `map.bin`     : the same encoded words re-chunked as 2048 chunks × 32
//   tiles × u16 BIG-endian, addressed by `Chunk::mapChunkAddr` (64 bytes per
//   chunk) on the device. `raw_map.bin` and `map.bin` always carry identical
//   words; only the byte order and tile ordering differ.
// - `scripts.bin` : 2048 chunks × 128 bytes, each non-empty chunk is the
//   concatenated compiled bytecode followed by an End (0xFF) sentinel.
// - `text.bin`    : u16 LE count + u16 LE offsets + u16 LE len-prefixed
//   strings.
// - `.hpp` files : opcodes / flags / flag bit-array / locations / scripts
//   in Go's exact layout (sorted names; scripts.hpp embeds the raw script
//   sources as comments and the compiled blob (no End) as byte arrays).
//
// The script compiler is `crate::script` (ported from Go internal/script);
// `@name` location references resolve against an EMPTY map exactly like Go
// mapbuilder.go, which builds `locations := map[string][2]uint16{}`.
// ─────────────────────────────────────────────────────────────────────

/// Chunk payload size in `scripts.bin` (Go `chunkSize = 128`).
const CHUNK_SIZE: usize = 128;

/// Every artifact a map build produces (mirrors the files Go `BuildMap`
/// writes into the output directory).
#[derive(Debug)]
pub struct MapArtifacts {
    pub raw_map: Vec<u8>,
    pub scripts_bin: Vec<u8>,
    pub text_bin: Vec<u8>,
    pub opcodes_hpp: String,
    pub flags_hpp: String,
    pub flag_bit_array_hpp: String,
    pub flag_bit_array_cpp: String,
    pub locations_hpp: String,
    pub scripts_hpp: String,
}

fn opcodes_hpp_text() -> String {
    String::from(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
         enum class VmOpcode : uint8_t {\n\
         \x20   Msg = 0,\n\
         \x20   TMsg = 1,\n\
         \x20   SMsg = 2,\n\
         \x20   Tp = 3,\n\
         \x20   TpIf = 4,\n\
         \x20   If = 5,\n\
         \x20   SetFlag = 6,\n\
         \x20   UnsetFlag = 7,\n\
         \x20   ReadFlag = 8,\n\
         \x20   End = 255,\n\
         };\n",
    )
}

/// Go `buildTextBin`: u16 LE count, u16 LE per-text offset, then each text
/// as u16 LE byte-length + bytes.
fn build_text_bin(texts: &[String]) -> Vec<u8> {
    let mut offsets = Vec::with_capacity(texts.len());
    let mut blob = Vec::new();
    let mut cursor: u16 = 0;
    for t in texts {
        offsets.push(cursor);
        let ascii = t.as_bytes();
        blob.extend_from_slice(&(ascii.len() as u16).to_le_bytes());
        blob.extend_from_slice(ascii);
        cursor += 2 + ascii.len() as u16;
    }

    let mut out = Vec::new();
    out.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
    for off in offsets {
        out.extend_from_slice(&off.to_le_bytes());
    }
    out.extend_from_slice(&blob);
    out
}

/// Go `writeFlagsHPP` / `writeLocationsHPP`: sorted names, one
/// `uint16_t name = id;` line each, header first.
fn name_id_hpp(header: &str, table: &HashMap<String, u16>, prefix: &str) -> String {
    let mut h = String::from(header);
    let mut names: Vec<&String> = table.keys().collect();
    names.sort();
    for name in names {
        h.push_str(&format!("{prefix}{name} = {};\n", table[name]));
    }
    h
}

/// Go `writeScriptsHPP`: per non-empty chunk, optional `// source` comment
/// (raw script text) then a `uint8_t blobN[] = { b0,b1,... };` line.
/// Note: the compiled blob does NOT include the trailing 0xFF End that is
/// appended only inside `scripts.bin`.
fn scripts_hpp_text(blobs: &[Vec<u8>], scripts: &[String]) -> String {
    let mut h =
        String::from("#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n");
    for (i, blob) in blobs.iter().enumerate() {
        if blob.is_empty() {
            continue;
        }
        if !scripts[i].is_empty() {
            h.push_str(&format!("// {}\n", scripts[i]));
        }
        let nums: Vec<String> = blob.iter().map(|b| b.to_string()).collect();
        h.push_str(&format!(
            "uint8_t blob{}[] = {{ {} }};\n",
            i,
            nums.join(",")
        ));
    }
    h
}

/// A Tiled location marker is the complete `@name` carried through
/// NPCObject.script by the importer. Other NPC scripts stay executable.
fn location_marker(script: &str) -> Option<&str> {
    let name = script.strip_prefix('@')?;
    (!name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_'))
    .then_some(name)
}

/// Mirrors Go `BuildMap`: compiles the map's NPC + script-object scripts
/// into per-chunk bytecode and produces every output artifact in memory.
pub fn build_map(map: &GameMap) -> Result<MapArtifacts, BuildError> {
    use crate::model::{CHUNK_COLS, CHUNK_H, CHUNK_W, MAP_H, MAP_W, TOTAL_CHUNKS};

    // raw_map.bin: 256×256 encoded tile words, row-major, little-endian.
    let mut raw_map = vec![0u8; (MAP_W * MAP_H * 2) as usize];
    for row in 0..MAP_H {
        for col in 0..MAP_W {
            let tile = map
                .tiles
                .get(row as usize)
                .and_then(|trow| trow.get(col as usize))
                .copied()
                .unwrap_or(0);
            if tile > 0x03FF {
                let path = map
                    .source_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<map>".to_string());
                return Err(BuildError(format!(
                    "{path}: tile ({col},{row}) has GID {tile}, maximum supported GID is 1023"
                )));
            }
            let properties = if tile == 0 {
                0
            } else {
                map.tile_properties.get(&tile).copied().unwrap_or(0)
            };
            if properties > 0x3F {
                let path = map
                    .source_path
                    .as_ref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<map>".to_string());
                return Err(BuildError(format!(
                    "{path}: tile ({col},{row}) GID {tile} has invalid property mask {properties}"
                )));
            }
            let encoded = tile | ((properties as u16) << 10);
            let off = ((row * MAP_W + col) * 2) as usize;
            raw_map[off] = (encoded & 0xFF) as u8;
            raw_map[off + 1] = (encoded >> 8) as u8;
        }
    }

    // Tiled locations are represented by NPCObject entries because GameMap
    // has no dedicated location-name field. A bare @name is metadata, not an
    // executable NPC script; make it available to script location resolution.
    let mut locations = HashMap::new();
    for npc in &map.npcs {
        if let Some(name) = location_marker(&npc.script) {
            let x = u16::try_from(npc.x).map_err(|_| {
                BuildError(format!(
                    "location @{name} has invalid x coordinate {}",
                    npc.x
                ))
            })?;
            let y = u16::try_from(npc.y).map_err(|_| {
                BuildError(format!(
                    "location @{name} has invalid y coordinate {}",
                    npc.y
                ))
            })?;
            locations.insert(name.to_owned(), (x, y));
        }
    }

    // Collect every non-empty executable script by chunk index (NPCs first,
    // then script objects). Location markers were consumed above.
    struct ScriptEntry {
        chunk_idx: usize,
        src: String,
    }
    let mut entries = Vec::new();
    for npc in &map.npcs {
        if !npc.script.is_empty() && location_marker(&npc.script).is_none() {
            let idx = ((npc.y / CHUNK_H) * CHUNK_COLS + npc.x / CHUNK_W) as usize;
            entries.push(ScriptEntry {
                chunk_idx: idx,
                src: npc.script.clone(),
            });
        }
    }
    for sc in &map.scripts {
        if !sc.script.is_empty() {
            let idx = ((sc.y / CHUNK_H) * CHUNK_COLS + sc.x / CHUNK_W) as usize;
            entries.push(ScriptEntry {
                chunk_idx: idx,
                src: sc.script.clone(),
            });
        }
    }

    let mut ctrl = script::Controller::new();

    let mut chunk_blobs: Vec<Vec<u8>> = vec![Vec::new(); TOTAL_CHUNKS];
    let mut chunk_scripts: Vec<String> = vec![String::new(); TOTAL_CHUNKS];
    for e in &entries {
        let cmds = script::parse(&e.src, &locations, &mut ctrl)
            .map_err(|err| BuildError(format!("parse error in chunk {}: {}", e.chunk_idx, err)))?;
        let mut blob = Vec::new();
        for cmd in &cmds {
            blob.extend_from_slice(&cmd.to_bytes());
        }
        chunk_blobs[e.chunk_idx].extend_from_slice(&blob);
        chunk_scripts[e.chunk_idx].push_str(&e.src);
    }

    // scripts.bin: 2048 chunks × 128 bytes (padded), each non-empty chunk
    // ends with an End (0xFF) sentinel.
    let mut scripts_bin = vec![0u8; TOTAL_CHUNKS * CHUNK_SIZE];
    for i in 0..TOTAL_CHUNKS {
        let mut blob = chunk_blobs[i].clone();
        if blob.is_empty() {
            continue;
        }
        blob.push(0xFF);
        if blob.len() > CHUNK_SIZE {
            return Err(BuildError(format!(
                "chunk {} too large: {} bytes > {}",
                i,
                blob.len(),
                CHUNK_SIZE
            )));
        }
        scripts_bin[i * CHUNK_SIZE..i * CHUNK_SIZE + blob.len()].copy_from_slice(&blob);
    }

    // text.bin uses the controller's deduplicated texts indexed by id.
    let mut sorted_texts: Vec<String> = vec![String::new(); ctrl.texts.len()];
    for (text, &idx) in &ctrl.texts {
        sorted_texts[idx as usize] = text.clone();
    }
    let text_bin = build_text_bin(&sorted_texts);

    let opcodes_hpp = opcodes_hpp_text();
    let flags_hpp = name_id_hpp(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n",
        &ctrl.flags,
        "uint16_t ",
    );
    let bits = ctrl.flags.len();
    let bytes = bits / 8 + usize::from(bits % 8 != 0);
    let flag_bit_array_hpp = format!(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nextern uint8_t FLAG_BIT_ARRAY[{bytes}];\n"
    );
    let flag_bit_array_cpp = format!(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nuint8_t FLAG_BIT_ARRAY[{bytes}] = {{0}};\n"
    );
    let locations_hpp = name_id_hpp(
        "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n",
        &ctrl.tags,
        "uint16_t ",
    );
    let scripts_hpp = scripts_hpp_text(&chunk_blobs, &chunk_scripts);

    Ok(MapArtifacts {
        raw_map,
        scripts_bin,
        text_bin,
        opcodes_hpp,
        flags_hpp,
        flag_bit_array_hpp,
        flag_bit_array_cpp,
        locations_hpp,
        scripts_hpp,
    })
}

/// Chunked big-endian `map.bin`: 2048 chunks × 32 tiles × u16 BE, the packed
/// `map_data` FX field. Re-chunks the `raw_map` words so both artifacts carry
/// the same encoded tile+property values; the device reads this field with
/// `Chunk::mapChunkAddr` and the chunk index is `(y/4)*32 + x/8`.
fn chunk_map_be(raw_map: &[u8]) -> Vec<u8> {
    use crate::model::{CHUNK_COLS, CHUNK_H, CHUNK_W, MAP_H, MAP_W, TOTAL_CHUNKS};

    let mut out = Vec::with_capacity(TOTAL_CHUNKS * (CHUNK_W * CHUNK_H) as usize * 2);
    for chunk_y in 0..(MAP_H / CHUNK_H) {
        for chunk_x in 0..CHUNK_COLS {
            for tile_y in 0..CHUNK_H {
                for tile_x in 0..CHUNK_W {
                    let x = chunk_x * CHUNK_W + tile_x;
                    let y = chunk_y * CHUNK_H + tile_y;
                    let off = ((y * MAP_W + x) * 2) as usize;
                    let word = u16::from_le_bytes([raw_map[off], raw_map[off + 1]]);
                    out.extend_from_slice(&word.to_be_bytes());
                }
            }
        }
    }
    out
}

/// Mirrors Go `BuildMap` file side: writes all map artifacts into
/// `cfg.output_dir` (creating it if needed).
pub fn write_map(cfg: &Config, map: &GameMap) -> Result<(), BuildError> {
    let a = build_map(map)?;
    let dir = Path::new(&cfg.output_dir);
    std::fs::create_dir_all(dir).map_err(|e| io_err("creating", dir, e))?;
    write_bytes(&dir.join("raw_map.bin"), &a.raw_map)?;
    write_bytes(&dir.join("map.bin"), &chunk_map_be(&a.raw_map))?;
    write_bytes(&dir.join("scripts.bin"), &a.scripts_bin)?;
    write_bytes(&dir.join("text.bin"), &a.text_bin)?;
    write_str(&dir.join("opcodes.hpp"), &a.opcodes_hpp)?;
    write_str(&dir.join("flags.hpp"), &a.flags_hpp)?;
    write_str(&dir.join("flag_bit_array.hpp"), &a.flag_bit_array_hpp)?;
    write_str(&dir.join("flag_bit_array.cpp"), &a.flag_bit_array_cpp)?;
    write_str(&dir.join("locations.hpp"), &a.locations_hpp)?;
    write_str(&dir.join("scripts.hpp"), &a.scripts_hpp)?;
    Ok(())
}

fn write_bytes(path: &Path, data: &[u8]) -> Result<(), BuildError> {
    std::fs::write(path, data).map_err(|e| io_err("writing", path, e))
}

fn write_str(path: &Path, data: &str) -> Result<(), BuildError> {
    std::fs::write(path, data).map_err(|e| io_err("writing", path, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NPCObject, ScriptObject};
    use crate::store::Store;

    const TD: &str = env!("CARGO_MANIFEST_DIR");
    const CREATURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/creatures.json");
    const MOVES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/moves.json");

    fn fixture_cfg(output_dir: &str) -> Config {
        Config {
            creatures_json: CREATURES.to_string(),
            moves_json: MOVES.to_string(),
            map_json: concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/map.json").to_string(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: format!("{TD}/testdata/builder_out/{output_dir}"),
            fixture_dir: None,
            battle_presets: None,
            arena_demo: None,
            duel: None,
            fx_layout: None,
            blocks_json: None,
        }
    }

    fn fixture_creatures() -> Vec<Creature> {
        let cfg = fixture_cfg("_");
        Store::new(&cfg).load_creatures().expect("fixture parses")
    }

    fn fixture_moves() -> Vec<Move> {
        let cfg = fixture_cfg("_");
        Store::new(&cfg).load_moves().expect("fixture parses")
    }

    const TYPE_TABLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/typetable.csv");
    // Exact output from `tools/data_converters/type_table_data.py --format fx`.
    const EXPECTED_TYPE_TABLE_BIN: [u8; 64] = [
        3, 3, 3, 3, 3, 3, 3, 0, 3, 3, 4, 2, 4, 3, 2, 2, 3, 3, 3, 4, 2, 2, 4, 2, 3, 4, 2, 3, 3, 4,
        2, 2, 3, 0, 4, 2, 3, 4, 4, 3, 3, 4, 4, 0, 4, 3, 2, 3, 3, 4, 4, 4, 0, 2, 3, 3, 4, 3, 3, 3,
        3, 3, 3, 4,
    ];
    const TYPE_TABLE_PYTHON_COMMAND: &str =
        "python3 tools/data_converters/type_table_data.py --format fx";
    const TYPE_TABLE_RUST_COMMAND: &str = "cgfx-tools --project cgfx-project.json";

    const CONSUMABLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/consumables.csv");

    #[test]
    fn consumables_build_as_eight_kind_arg_records_in_id_order() {
        let table = load_consumables_csv(CONSUMABLES).expect("consumables fixture parses");
        assert_eq!(
            table.rows[0],
            ConsumableDef {
                kind: ConsumableKind::Heal,
                arg: 16
            }
        );
        assert_eq!(
            table.rows[7],
            ConsumableDef {
                kind: ConsumableKind::Charge,
                arg: 32
            }
        );
        assert_eq!(
            build_consumables(&table),
            [1, 16, 1, 32, 1, 64, 2, 1, 2, 2, 2, 3, 3, 16, 3, 32]
        );
    }

    #[test]
    fn consumables_reject_wrong_row_count_duplicate_id_and_unknown_kind() {
        let valid = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/consumables.csv"
        ));
        let too_few = valid.lines().take(9).collect::<Vec<_>>().join("\n");
        assert!(
            parse_consumables_csv_source(&too_few, "too_few.csv")
                .expect_err("seven records must fail")
                .to_string()
                .contains("expected 8 rows, found 7")
        );

        let duplicate = valid.replace("7,Charge,32", "0,Charge,32");
        assert!(
            parse_consumables_csv_source(&duplicate, "duplicate.csv")
                .expect_err("duplicate id must fail")
                .to_string()
                .contains("duplicate consumable id 0")
        );

        let unknown = valid.replace("7,Charge,32", "7,Unknown,32");
        assert!(
            parse_consumables_csv_source(&unknown, "unknown.csv")
                .expect_err("unknown kind must fail")
                .to_string()
                .contains("unknown consumable kind")
        );

        let gap = valid.replace("7,Charge,32", "8,Charge,32");
        assert!(
            parse_consumables_csv_source(&gap, "gap.csv")
                .expect_err("out-of-range id leaves a gap and must fail")
                .to_string()
                .contains("outside 0..7")
        );
    }

    #[test]
    fn build_type_table_matches_python_layout() {
        let table = load_type_table(TYPE_TABLE).expect("type table fixture parses");
        assert_eq!(table.rows.len(), 8);
        assert_eq!(table.rows[0].name, "spirit");
        assert_eq!(table.rows[0].modifiers.len(), 8);
        assert_eq!(build_type_table(&table), EXPECTED_TYPE_TABLE_BIN);
    }

    #[test]
    fn write_type_table_emits_typetable_bin() {
        let table = load_type_table(TYPE_TABLE).expect("type table fixture parses");
        let output = format!("{TD}/testdata/builder_out/type_table");
        write_type_table(&output, &table).expect("type table writes");
        assert_eq!(
            std::fs::read(format!("{output}/typetable.bin")).expect("read typetable.bin"),
            EXPECTED_TYPE_TABLE_BIN,
        );
    }

    #[test]
    fn byte_parity_reports_injected_type_table_divergence() {
        let table = load_type_table(TYPE_TABLE).expect("type table fixture parses");
        let mut actual = build_type_table(&table);
        actual[17] = 0xFF;
        let error = verify_byte_parity(
            "typetable.bin",
            &EXPECTED_TYPE_TABLE_BIN,
            &actual,
            TYPE_TABLE_PYTHON_COMMAND,
            TYPE_TABLE_RUST_COMMAND,
        )
        .expect_err("one-byte divergence must fail parity");
        assert_eq!(
            error.to_string(),
            "typetable.bin parity mismatch at byte 17: expected 0x03 from `python3 tools/data_converters/type_table_data.py --format fx`, got 0xFF from `cgfx-tools --project cgfx-project.json`"
        );
    }

    // ─── creatures.bin: exact bytes from the Go builder (reference contract) ───
    // SquibbleSnail: wind=2, none=8 -> {0,2,8,8,3,3,2,2,3,3,8,32,32,32}
    // NewOne:        none=8, none=8 -> {99,8,8,1,1,1,1,1,1,1,0,0,0,0}
    const EXPECTED_CREATURES_BIN: [u8; 28] = [
        0, 2, 8, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32, //
        99, 8, 8, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0,
    ];

    const EXPECTED_CREATURES_HPP: &str = r#"namespace CreatureData {

    uint8_t creatureData[] = {
    {0, 2, 8, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32},
    {99, 8, 8, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0},
    };

}
"#;

    #[test]
    fn build_creatures_bin_size() {
        let creatures = fixture_creatures();
        let (bin, _) = build_creatures(&creatures).expect("build");
        assert_eq!(bin.len(), creatures.len() * 14);
    }

    #[test]
    fn build_creatures_bin_matches_go_reference() {
        let creatures = fixture_creatures();
        let (bin, _) = build_creatures(&creatures).expect("build");
        assert_eq!(bin, EXPECTED_CREATURES_BIN.to_vec());
    }

    #[test]
    fn build_creatures_first_entry() {
        let creatures = fixture_creatures();
        let (bin, _) = build_creatures(&creatures).expect("build");
        // SquibbleSnail (Go TestBuildCreaturesFirstEntry).
        let expected: [u8; 14] = [0, 2, 8, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32];
        assert_eq!(&bin[..14], &expected);
    }

    #[test]
    fn build_creatures_hpp_contains() {
        let creatures = fixture_creatures();
        let (_, hpp) = build_creatures(&creatures).expect("build");
        assert!(hpp.contains("creatureData[]"));
    }

    #[test]
    fn build_creatures_hpp_matches_go_reference() {
        let creatures = fixture_creatures();
        let (_, hpp) = build_creatures(&creatures).expect("build");
        assert_eq!(hpp, EXPECTED_CREATURES_HPP);
    }

    #[test]
    fn build_creatures_unknown_type_errors() {
        let mut creatures = fixture_creatures();
        creatures[0].type1 = "bogus".to_string();
        let err = build_creatures(&creatures).expect_err("must error");
        assert!(err.0.contains("unknown type for creature SquibbleSnail"));
    }

    #[test]
    fn build_creatures_hp_bound_accepts_level_31_boundary() {
        // 2*31 + 16*(31/3) + 30 = 252 <= 254.
        let mut creatures = fixture_creatures();
        creatures[0].hp = 16;
        assert!(build_creatures(&creatures).is_ok());
    }

    #[test]
    fn build_creatures_hp_bound_rejects_over_254() {
        // 2*31 + 17*(31/3) + 30 = 262 > 254.
        let mut creatures = fixture_creatures();
        creatures[0].hp = 17;
        let err = build_creatures(&creatures).expect_err("hp bound must error");
        assert!(err.0.contains("above 254"), "{err}");
    }

    #[test]
    fn build_creatures_hp_seed_range_errors() {
        let mut creatures = fixture_creatures();
        creatures[0].hp = -1;
        let err = build_creatures(&creatures).expect_err("negative hp seed");
        assert!(err.0.contains("0..=255"), "{err}");
        creatures[0].hp = 256;
        let err = build_creatures(&creatures).expect_err("hp seed above byte range");
        assert!(err.0.contains("0..=255"), "{err}");
    }

    // ─── moves.bin: exact bytes from the Go builder (reference contract) ───
    // smite:   type spirit=0, power=10, physical=0, acc=100 -> packed=1280=0x0500
    //          -> [0x05, 0x00, 0xFF(eff NONE), 0xFF(effect2)]
    // thought: type spirit=0, power=5,  physical=1, acc=100 -> packed=704=0x02C0
    //          -> [0x02, 0xC0, 0xFF, 0xFF]
    const EXPECTED_MOVES_BIN: [u8; 8] = [0x05, 0x00, 0xFF, 0xFF, 0x02, 0xC0, 0xFF, 0xFF];

    const EXPECTED_MOVES_HPP: &str = r#"namespace MoveData {
    uint8_t move0[] = "smite\0";
    uint8_t move1[] = "thought\0";

    uint24_t moveNames[] = {
        move0, move1
    };

}
"#;

    #[test]
    fn build_moves_bin_size() {
        let moves = fixture_moves();
        let (bin, _) = build_moves(&moves).expect("build");
        assert_eq!(bin.len(), moves.len() * 4);
    }

    #[test]
    fn build_moves_bin_matches_go_reference() {
        let moves = fixture_moves();
        let (bin, _) = build_moves(&moves).expect("build");
        assert_eq!(bin, EXPECTED_MOVES_BIN.to_vec());
    }

    #[test]
    fn build_moves_hpp_contains() {
        let moves = fixture_moves();
        let (_, hpp) = build_moves(&moves).expect("build");
        assert!(hpp.contains("move0[]"));
        assert!(hpp.contains("moveNames[]"));
    }

    #[test]
    fn build_moves_hpp_matches_go_reference() {
        let moves = fixture_moves();
        let (_, hpp) = build_moves(&moves).expect("build");
        assert_eq!(hpp, EXPECTED_MOVES_HPP);
    }

    #[test]
    fn build_moves_unknown_type_errors() {
        let mut moves = fixture_moves();
        moves[0].r#type = "void".to_string();
        let err = build_moves(&moves).expect_err("must error");
        assert!(err.0.contains("unknown move type \"void\" for move smite"));
    }

    #[test]
    fn build_moves_none_type_is_15() {
        let moves = vec![Move {
            id: 7,
            name: "stall".to_string(),
            r#type: "none".to_string(),
            power: 0,
            physical: 0,
            accuracy: 100,
            effect: "NONE".to_string(),
        }];
        let (bin, _) = build_moves(&moves).expect("build");
        // packed = 15<<12 | 0<<7 | 0<<6 | 0<<4 = 0xF000
        assert_eq!(bin, vec![0xF0, 0x00, 0xFF, 0xFF]);
    }

    #[test]
    fn build_moves_known_effect_soaked() {
        let moves = vec![Move {
            id: 2,
            name: "splash".to_string(),
            r#type: "water".to_string(),
            power: 3,
            physical: 0,
            accuracy: 90,
            effect: "SOAKED".to_string(),
        }];
        let (bin, _) = build_moves(&moves).expect("build");
        // t=1 -> 1<<12=0x1000; power 3<<7=0x180; acc 90->1 <<4=0x10 -> 0x1190
        assert_eq!(bin[..2], [0x11, 0x90]);
        assert_eq!(bin[2], 1); // SOAKED
        assert_eq!(bin[3], 0xFF); // effect2
    }

    #[test]
    fn build_moves_unknown_effect_defaults_ff() {
        let mut moves = fixture_moves();
        moves[0].effect = "MYSTERY".to_string();
        let (bin, _) = build_moves(&moves).expect("build");
        assert_eq!(bin[2], 0xFF);
    }

    #[test]
    fn build_moves_unknown_accuracy_defaults_zero() {
        let mut moves = fixture_moves();
        moves[0].accuracy = 55; // not in Go accuracyIndex
        let (bin, _) = build_moves(&moves).expect("build");
        // smite power=10, physical=0: packed = 0<<12|10<<7|0<<6|0<<4 = 0x0500
        assert_eq!(bin[..2], [0x05, 0x00]);
    }

    // ─── Go-style file writing into cfg.output_dir ───
    #[test]
    fn write_creatures_outputs_files() {
        let cfg = fixture_cfg("creatures");
        let creatures = fixture_creatures();
        write_creatures(&cfg, &creatures).expect("write");
        let bin = std::fs::read(format!("{}/creatureData.bin", cfg.output_dir)).expect("read bin");
        let hpp = std::fs::read_to_string(format!("{}/creatureData.hpp", cfg.output_dir))
            .expect("read hpp");
        assert_eq!(bin, EXPECTED_CREATURES_BIN.to_vec());
        assert_eq!(hpp, EXPECTED_CREATURES_HPP);
    }

    #[test]
    fn write_moves_outputs_files() {
        let cfg = fixture_cfg("moves");
        let moves = fixture_moves();
        write_moves(&cfg, &moves).expect("write");
        let bin = std::fs::read(format!("{}/moves.bin", cfg.output_dir)).expect("read bin");
        let hpp =
            std::fs::read_to_string(format!("{}/moves.hpp", cfg.output_dir)).expect("read hpp");
        let txt =
            std::fs::read_to_string(format!("{}/moves.txt", cfg.output_dir)).expect("read txt");
        assert_eq!(bin, EXPECTED_MOVES_BIN.to_vec());
        assert_eq!(hpp, EXPECTED_MOVES_HPP);
        assert_eq!(txt, EXPECTED_MOVES_HPP, "moves.txt mirrors moves.hpp bytes");
    }

    #[test]
    fn build_moves_uses_canonical_id_symbols_for_declaration_and_pointers() {
        // Canonical `data/json/moves.json` gives Deluge id 44 even though it sits
        // at the 33rd roster position; the emitted symbol must use that id in both
        // the declaration and the pointer table.
        let moves = vec![
            Move {
                id: 0,
                name: "smite".to_string(),
                r#type: "spirit".to_string(),
                power: 10,
                physical: 0,
                accuracy: 100,
                effect: "NONE".to_string(),
            },
            Move {
                id: 44,
                name: "Deluge".to_string(),
                r#type: "none".to_string(),
                power: 11,
                physical: 0,
                accuracy: 100,
                effect: "NONE".to_string(),
            },
        ];
        let (_, hpp) = build_moves(&moves).expect("build");
        assert!(
            hpp.contains("    uint8_t move44[] = \"Deluge\\0\";\n"),
            "Deluge must declare the canonical move{{id}} symbol:\n{hpp}"
        );
        assert!(
            !hpp.contains("move32"),
            "canonical emission must not fall back to roster-position symbols:\n{hpp}"
        );
        assert!(
            hpp.contains("        move0, move44\n"),
            "pointer table must reference the canonical symbol:\n{hpp}"
        );
    }

    // ─── FX text descriptors consumed by fxdata-build.py ───
    #[test]
    fn fx_creature_descriptor_matches_python_format() {
        let descriptor = build_fx_creature_descriptor(&fixture_creatures()).expect("build");
        assert_eq!(
            descriptor,
            "// Generated from data/Creaturesheet.csv by tools/data_converters/creature_data.py.\n\
             // Do not edit: regenerate with python3 tools/data_converters/creature_data.py.\n\
             namespace CreatureData {\n\nuint8_t creatureData[] = {\n\
             \x20   {0, 2, 15, 8, 3, 3, 2, 2, 3, 3, 8, 32, 32, 32},\n\
             \x20   {99, 15, 15, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0},\n\
             };\n}\nnamespace_end\n"
        );
    }

    #[test]
    fn fx_move_descriptor_matches_python_format() {
        let descriptor = build_fx_move_descriptor(&fixture_moves()).expect("build");
        assert_eq!(
            descriptor,
            "namespace MoveData {\nuint8_t movePack[] = {\n\
             { 0, 10, 15, 100, 255 },\n\
             { 0, 5, 15, 100, 255 },\n\
             };\n} namespace_end\n"
        );
    }

    #[test]
    fn fx_opponents_descriptor_matches_python_format() {
        let opponents = load_opponents_csv(OPPONENTS_CSV).expect("fixture parses");
        assert_eq!(
            build_fx_opponents_descriptor(&opponents),
            "uint8_t opponent_seeds[] = {\n\
             \x20   0x00, 0x1F, 0x00, 0x01, 0xFF, 0xFF,\n\
             \x20   0xFF, 0x00, 0x17, 0xFF, 0xFF, 0xFF,\n\
             \x20   0x1D, 0xFF, 0x1F, 0xFF, 0xFF, 0xFF,\n\
             };\n\nuint24_t opts[] = {\n    opponent_seeds,\n};\n"
        );
    }

    #[test]
    fn write_fx_descriptors_emits_all_pack_inputs() {
        let output = format!("{TD}/testdata/builder_out/fx_descriptors");
        let creatures = fixture_creatures();
        let moves = fixture_moves();
        let opponents = load_opponents_csv(OPPONENTS_CSV).expect("fixture parses");
        write_fx_descriptors(&output, &creatures, &moves, &opponents).expect("write");
        assert_eq!(
            std::fs::read_to_string(format!("{output}/creatureData.txt")).expect("read creature"),
            build_fx_creature_descriptor(&creatures).expect("build")
        );
        assert_eq!(
            std::fs::read_to_string(format!("{output}/movedata.txt")).expect("read move"),
            build_fx_move_descriptor(&moves).expect("build")
        );
        assert_eq!(
            std::fs::read_to_string(format!("{output}/opponents.txt")).expect("read opponents"),
            build_fx_opponents_descriptor(&opponents)
        );
    }

    #[test]
    fn fx_descriptor_parity_checks_all_three_artifacts() {
        let reference = FxDescriptorReference {
            opponents: b"opponents".to_vec(),
            creatures: b"creatures".to_vec(),
            moves: b"moves".to_vec(),
        };
        let mut actual = reference.clone();
        actual.moves[0] = b'M';
        let error =
            verify_fx_descriptor_parity(&reference, &actual, "python3 reference", "cgfx-tools")
                .expect_err("move descriptor mismatch must fail");
        assert_eq!(
            error.to_string(),
            "movedata.txt parity mismatch at byte 0: expected 0x6D from `python3 reference`, got 0x4D from `cgfx-tools`"
        );
    }

    // ─── opponents.bin: Python opponent_data.py layout ───
    const OPPONENTS_CSV: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/opponents.csv");

    #[test]
    fn build_opponents_matches_known_good_vector() {
        let opponents = [OpponentSeed {
            first: OpponentPartyMember {
                creature_id: 0,
                level: 31,
                moves: 0xFFFF_0100,
            },
            second: OpponentPartyMember {
                creature_id: 255,
                level: 0,
                moves: 0xFFFF_FF17,
            },
            third: OpponentPartyMember {
                creature_id: 29,
                level: 255,
                moves: 0xFFFF_FF1F,
            },
        }];
        assert_eq!(
            build_opponents(&opponents),
            vec![
                0, 31, 0, 1, 0xFF, 0xFF, 0xFF, 0, 23, 0xFF, 0xFF, 0xFF, 29, 0xFF, 31, 0xFF, 0xFF,
                0xFF,
            ]
        );
    }

    #[test]
    fn load_opponents_csv_uses_python_name_maps() {
        let opponents = load_opponents_csv(OPPONENTS_CSV).expect("fixture parses");
        assert_eq!(opponents.len(), 1);
        assert_eq!(
            opponents[0].first,
            OpponentPartyMember {
                creature_id: 0,
                level: 31,
                moves: 0xFFFF_0100
            }
        );
        assert_eq!(
            opponents[0].second,
            OpponentPartyMember {
                creature_id: 255,
                level: 0,
                moves: 0xFFFF_FF17
            }
        );
        assert_eq!(
            opponents[0].third,
            OpponentPartyMember {
                creature_id: 29,
                level: 255,
                moves: 0xFFFF_FF1F
            }
        );
    }

    #[test]
    fn canonical_opponent_loader_preserves_legacy_fixture_bytes() {
        let legacy = load_opponents_csv(OPPONENTS_CSV).expect("legacy fixture parses");
        let canonical_moves = [
            canonical_move(0, "smite"),
            canonical_move(1, "thought"),
            canonical_move(23, "zap"),
            canonical_move(31, "elderSlam"),
        ];
        let canonical = load_opponents_csv_with_moves(OPPONENTS_CSV, &canonical_moves)
            .expect("canonical fixture parses");
        assert_eq!(build_opponents(&canonical), build_opponents(&legacy));
    }

    fn canonical_move(id: i32, name: &str) -> Move {
        Move {
            id,
            name: name.to_owned(),
            r#type: "water".to_owned(),
            power: 0,
            physical: 0,
            accuracy: 100,
            effect: String::new(),
        }
    }

    fn write_test_opponents(contents: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "cgfx-opponents-{}-{}.csv",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, contents).expect("write opponent test csv");
        path
    }

    #[test]
    fn canonical_opponent_loader_resolves_new_names_and_packs_little_endian() {
        let csv = format!(
            "{}\nsquibblesnail,none,none,31,0,0,fixture,deepthought,Ironbody,Sharpen,rejuvinate,none,none,none,none,none,none,none,none\n",
            "c1,c2,c3,l1,l2,l3,name,c1m1,c1m2,c1m3,c1m4,c2m1,c2m2,c2m3,c2m4,c3m1,c3m2,c3m3,c3m4"
        );
        let path = write_test_opponents(&csv);
        let moves = [
            canonical_move(42, "deepthought"),
            canonical_move(41, "Ironbody"),
            canonical_move(37, "Sharpen"),
            canonical_move(43, "rejuvinate"),
            canonical_move(43, "none"),
        ];
        let opponents =
            load_opponents_csv_with_moves(&path, &moves).expect("canonical names parse");
        std::fs::remove_file(&path).expect("remove opponent test csv");

        assert_eq!(opponents.len(), 1);
        assert_eq!(opponents[0].first.moves, 0x2B_25_29_2A);
        assert_eq!(opponents[0].second.moves, 0xFFFF_FFFF);
        assert_eq!(opponents[0].third.moves, 0xFFFF_FFFF);
    }

    #[test]
    fn canonical_opponent_loader_reports_path_line_member_and_move_slot() {
        let csv = "c1,c2,c3,l1,l2,l3,name,c1m1,c1m2,c1m3,c1m4,c2m1,c2m2,c2m3,c2m4,c3m1,c3m2,c3m3,c3m4\nsquibblesnail,none,none,31,0,0,fixture,known,Missing,none,none,none,none,none,none,none,none,none,none\n";
        let path = write_test_opponents(csv);
        let err = load_opponents_csv_with_moves(&path, &[canonical_move(3, "known")])
            .expect_err("unknown move rejected");
        std::fs::remove_file(&path).expect("remove opponent test csv");
        assert!(err.0.contains(&path.display().to_string()), "{err}");
        assert!(err.0.contains(":2: first party member:"), "{err}");
        assert!(
            err.0.contains("moves[2] has unknown move \"Missing\""),
            "{err}"
        );
    }

    #[test]
    fn canonical_opponent_loader_rejects_ambiguous_names_and_reserved_ids() {
        let csv = "c1,c2,c3,l1,l2,l3,name,c1m1,c1m2,c1m3,c1m4,c2m1,c2m2,c2m3,c2m4,c3m1,c3m2,c3m3,c3m4\nsquibblesnail,none,none,31,0,0,fixture,same,none,none,none,none,none,none,none,none,none,none,none\n";
        let path = write_test_opponents(csv);
        let ambiguous = [canonical_move(1, "same"), canonical_move(2, "same")];
        let err =
            load_opponents_csv_with_moves(&path, &ambiguous).expect_err("ambiguous name rejected");
        assert!(err.0.contains("ambiguous move name \"same\""), "{err}");
        let out_of_range = [canonical_move(255, "same")];
        let err =
            load_opponents_csv_with_moves(&path, &out_of_range).expect_err("reserved ID rejected");
        assert!(err.0.contains("invalid canonical move ID 255"), "{err}");
        let too_large = [canonical_move(300, "same")];
        let err =
            load_opponents_csv_with_moves(&path, &too_large).expect_err("ID beyond u8 rejected");
        assert!(err.0.contains("invalid canonical move ID 300"), "{err}");
        std::fs::remove_file(&path).expect("remove opponent test csv");
    }

    #[test]
    fn opponent_parity_reports_first_difference_and_commands() {
        let err = compare_opponent_bytes(
            &[0xAA, 0xBB],
            &[0xAA, 0xCC],
            "python3 opponent_data.py --format fx",
            "cgfx-tools --opponents-csv data/opponents.csv --opponents-output out",
        )
        .expect_err("injected divergence must fail");
        assert!(err.0.contains("offset 1"), "{err}");
        assert!(err.0.contains("expected 0xBB"), "{err}");
        assert!(err.0.contains("actual 0xCC"), "{err}");
        assert!(
            err.0.contains("python3 opponent_data.py --format fx"),
            "{err}"
        );
        assert!(err.0.contains("cgfx-tools --opponents-csv"), "{err}");
    }

    // ─── map builder: ported mapbuilder_test.go (reference contract) ───

    fn empty_map() -> GameMap {
        GameMap {
            width: 256,
            height: 256,
            tiles: vec![vec![0u16; 256]; 256],
            tilesets: vec![],
            tile_properties: Default::default(),
            source_path: None,
            npcs: vec![],
            scripts: vec![],
            collision: vec![],
            collision_types: vec![],
            encounters: vec![],
            encounter_zones: vec![],
            wfc: None,
            stamps: vec![],
        }
    }

    #[test]
    fn build_map_empty_raw_bin_all_zero() {
        let a = build_map(&empty_map()).expect("build");
        assert_eq!(a.raw_map.len(), 131072, "expected 131072 bytes");
        assert!(a.raw_map.iter().all(|&b| b == 0), "expected all zeros");
    }

    #[test]
    fn build_map_raw_bin_tiles_at_positions() {
        let mut m = empty_map();
        m.tiles[0][0] = 0x0102;
        m.tiles[1][2] = 0x0203;
        m.tiles[7][255] = 0x03FF;
        let a = build_map(&m).expect("build");
        // LE row-major: offset (row*256+col)*2.
        let off = |row: usize, col: usize| (row * 256 + col) * 2;
        assert_eq!(&a.raw_map[off(0, 0)..off(0, 0) + 2], &[0x02, 0x01]);
        assert_eq!(&a.raw_map[off(1, 2)..off(1, 2) + 2], &[0x03, 0x02]);
        assert_eq!(&a.raw_map[off(7, 255)..off(7, 255) + 2], &[0xFF, 0x03]);
    }

    #[test]
    fn build_map_encodes_six_tile_properties_and_gid_boundaries() {
        let mut m = empty_map();
        m.tiles[0][0] = 0;
        m.tiles[0][1] = 527;
        m.tiles[0][2] = 1023;
        m.tile_properties.insert(0, 0x3F);
        m.tile_properties.insert(527, 0x2A);
        m.tile_properties.insert(1023, 0x3F);

        let a = build_map(&m).expect("build");
        assert_eq!(a.raw_map.len(), 131072);
        assert_eq!(&a.raw_map[..2], &[0, 0], "GID zero stays an empty word");
        assert_eq!(&a.raw_map[2..4], &[0x0F, 0xAA], "527 plus mask 0x2A");
        assert_eq!(
            &a.raw_map[4..6],
            &[0xFF, 0xFF],
            "1023 plus all property bits"
        );
    }

    #[test]
    fn build_map_rejects_gid_over_tile_word_limit_with_coordinate() {
        let mut m = empty_map();
        m.tiles[2][3] = 1024;
        let error = build_map(&m).expect_err("over-limit GID must fail");
        assert!(error.0.contains("tile (3,2) has GID 1024"), "{error}");
    }

    #[test]
    fn build_map_out_of_bounds_tiles_are_zero() {
        // Go: raw map iterates the fixed 256×256 grid; rows/cols beyond the
        // tile matrix read as 0.
        let mut m = empty_map();
        m.tiles = vec![vec![9u16; 3]; 2]; // 2×3 map inside a 256×256 grid
        let a = build_map(&m).expect("build");
        assert_eq!(a.raw_map.len(), 131072);
        assert_eq!(&a.raw_map[..2], &[9, 0]);
        assert_eq!(&a.raw_map[2..4], &[9, 0]);
        assert_eq!(&a.raw_map[2 * 256 * 2..2 * 256 * 2 + 2], &[0, 0]);
        assert_eq!(&a.raw_map[255 * 256 * 2..255 * 256 * 2 + 2], &[0, 0]);
    }

    #[test]
    fn build_map_chunk_index_00_msg_opcode() {
        // Go TestChunkIndex00: script at (0,0) lands in chunk 0; "msg {Hi}"
        // compiles to [0, 0, 0] + 0xFF End.
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        let chunk_base = 0 * CHUNK_SIZE;
        assert_eq!(
            a.scripts_bin[chunk_base], 0,
            "expected Msg opcode 0 at chunk 0"
        );
        assert_eq!(&a.scripts_bin[chunk_base..chunk_base + 4], &[0, 0, 0, 0xFF]);
    }

    #[test]
    fn build_map_chunk_index_33_setflag_opcode() {
        // Go TestChunkIndex33: (8,4) → (8/8)*32 + (4/4) = 33.
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 8,
            y: 4,
            script: "setflag flag_myflag".to_string(),
        }];
        let a = build_map(&m).expect("build");
        let chunk_base = 33 * CHUNK_SIZE;
        assert_eq!(
            a.scripts_bin[chunk_base], 6,
            "expected SetFlag opcode 6 at chunk 33"
        );
        assert_eq!(&a.scripts_bin[chunk_base..chunk_base + 4], &[6, 0, 0, 0xFF]);
    }

    #[test]
    fn build_map_scripts_bin_multiple_of_128() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hello world}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        assert_eq!(a.scripts_bin.len() % 128, 0, "not multiple of 128");
        assert_eq!(a.scripts_bin.len(), 2048 * 128, "expected 262144 bytes");
    }

    #[test]
    fn build_map_npc_script_collected_before_script_objects() {
        // NPCs are appended to chunks before script objects (Go order),
        // so both compile into the same chunk blob in that order.
        let mut m = empty_map();
        m.npcs = vec![NPCObject {
            id: 1,
            x: 0,
            y: 0,
            sprite_id: 1,
            script: "msg {Hi}".to_string(),
        }];
        m.scripts = vec![ScriptObject {
            id: 2,
            x: 0,
            y: 0,
            script: "setflag flag_door".to_string(),
        }];
        let a = build_map(&m).expect("build");
        // msg bytes + setflag bytes + End: [0,0,0] [6,0,0] [0xFF]
        assert_eq!(&a.scripts_bin[..7], &[0, 0, 0, 6, 0, 0, 0xFF]);
    }

    #[test]
    fn build_map_empty_scripts_skipped() {
        let mut m = empty_map();
        m.scripts = vec![
            ScriptObject {
                id: 1,
                x: 0,
                y: 0,
                script: String::new(),
            },
            ScriptObject {
                id: 2,
                x: 0,
                y: 0,
                script: "msg {Hi}".to_string(),
            },
        ];
        m.npcs = vec![NPCObject {
            id: 3,
            x: 0,
            y: 0,
            sprite_id: 1,
            script: "".to_string(),
        }];
        let a = build_map(&m).expect("build");
        assert_eq!(&a.scripts_bin[..4], &[0, 0, 0, 0xFF]);
        assert_eq!(a.scripts_bin[4], 0, "rest of chunk stays zero-padded");
    }

    #[test]
    fn build_map_text_bin_matches_go_layout() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        // count=1 LE, offset[0]=0 LE, len=2 LE, "Hi"
        assert_eq!(a.text_bin, vec![1, 0, 0, 0, 2, 0, 0x48, 0x69]);
    }

    #[test]
    fn build_map_text_bin_offsets_accumulate() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {ab}; msg {cd}".to_string(),
        }];
        let a = build_map(&m).expect("build");
        // count=2; text0 offset 0; text1 offset 2+2=4; then len/bytes pairs
        // (lengths are u16 LE: len=2 -> [2, 0]).
        assert_eq!(
            a.text_bin,
            vec![2, 0, 0, 0, 4, 0, 2, 0, b'a', b'b', 2, 0, b'c', b'd']
        );
    }

    #[test]
    fn build_map_hpp_outputs_match_go() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        m.npcs = vec![NPCObject {
            id: 2,
            x: 8,
            y: 4,
            sprite_id: 1,
            script: "setflag flag_myflag".to_string(),
        }];
        let a = build_map(&m).expect("build");

        assert_eq!(
            a.opcodes_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
             enum class VmOpcode : uint8_t {\n\
             \x20   Msg = 0,\n\
             \x20   TMsg = 1,\n\
             \x20   SMsg = 2,\n\
             \x20   Tp = 3,\n\
             \x20   TpIf = 4,\n\
             \x20   If = 5,\n\
             \x20   SetFlag = 6,\n\
             \x20   UnsetFlag = 7,\n\
             \x20   ReadFlag = 8,\n\
             \x20   End = 255,\n\
             };\n"
        );
        // flags.hpp: single flag, sorted, `uint16_t name = id;`.
        assert_eq!(
            a.flags_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nuint16_t flag_myflag = 0;\n"
        );
        // locations.hpp: no tag used → header only.
        assert_eq!(
            a.locations_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n"
        );
        // flag bit array: 1 flag → 1 byte.
        assert_eq!(
            a.flag_bit_array_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nextern uint8_t FLAG_BIT_ARRAY[1];\n"
        );
        assert_eq!(
            a.flag_bit_array_cpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\nuint8_t FLAG_BIT_ARRAY[1] = {0};\n"
        );
        // scripts.hpp: chunk 0 (msg blob, no End) with source comment.
        assert_eq!(
            a.scripts_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
             // msg {Hi}\n\
             uint8_t blob0[] = { 0,0,0 };\n\
             // setflag flag_myflag\n\
             uint8_t blob33[] = { 6,0,0 };\n"
        );
    }

    #[test]
    fn build_map_parse_error_reports_chunk() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "nope".to_string(),
        }];
        let err = build_map(&m).expect_err("must fail");
        assert!(err.0.contains("parse error in chunk 0"), "err: {}", err.0);
    }

    #[test]
    fn build_map_chunk_too_large_errors() {
        let mut m = empty_map();
        // Each `msg {xxxxxxxxxx}` compiles to 3 bytes; 45 of them (135 bytes)
        // plus the 0xFF End sentinel overflow the 128-byte chunk capacity
        // (Go behavior).
        let script = std::iter::repeat_n("msg {xxxxxxxxxx}".to_string(), 45)
            .collect::<Vec<_>>()
            .join("; ");
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script,
        }];
        let err = build_map(&m).expect_err("must fail");
        assert!(err.0.contains("chunk 0 too large"), "err: {}", err.0);
    }

    #[test]
    fn write_map_emits_chunked_big_endian_map_bin() {
        let mut m = empty_map();
        m.tiles[0][0] = 7; // chunk 0, first tile
        m.tiles[3][7] = 0x03FF; // chunk 0, last tile (tile_y 3, tile_x 7)
        m.tiles[0][8] = 0x0102; // chunk 1, first tile
        m.tiles[6][0] = 5; // chunk index 1*32 + 0 = 32, tile_y 2 -> offset 32*64 + 32
        m.tile_properties.insert(7, 0x20); // encoded property must survive
        let cfg = fixture_cfg("map_chunks");
        write_map(&cfg, &m).expect("write");
        let map = std::fs::read(format!("{}/map.bin", cfg.output_dir)).expect("read map.bin");
        let raw = std::fs::read(format!("{}/raw_map.bin", cfg.output_dir)).expect("read raw");

        assert_eq!(map.len(), 131072, "2048 chunks x 32 tiles x 2 bytes");
        // Chunk 0 first word: tile 7 | (0x20 << 10) = 0x8007, big-endian.
        assert_eq!(&map[0..2], &[0x80, 0x07]);
        // Chunk 0 last word (within-chunk tile 31): 0x03FF.
        assert_eq!(&map[62..64], &[0x03, 0xFF]);
        // Chunk 1 first word (offset 1*64): 0x0102.
        assert_eq!(&map[64..66], &[0x01, 0x02]);
        // Chunk 32 first word (offset 32*64 + tile 16*2): 5.
        assert_eq!(&map[32 * 64 + 32..32 * 64 + 34], &[0x00, 0x05]);
        // Every chunk word is the big-endian form of the matching raw_map word.
        for chunk in 0..2048usize {
            for tile in 0..32usize {
                let x = (chunk % 32) * 8 + tile % 8;
                let y = (chunk / 32) * 4 + tile / 8;
                let raw_off = (y * 256 + x) * 2;
                let expected = u16::from_le_bytes([raw[raw_off], raw[raw_off + 1]]);
                let packed = chunk * 64 + tile * 2;
                assert_eq!(
                    u16::from_be_bytes([map[packed], map[packed + 1]]),
                    expected,
                    "chunk {chunk} tile {tile}",
                );
            }
        }
    }

    #[test]
    fn write_map_outputs_files() {
        let mut m = empty_map();
        m.scripts = vec![ScriptObject {
            id: 1,
            x: 0,
            y: 0,
            script: "msg {Hi}".to_string(),
        }];
        let cfg = fixture_cfg("map");
        let a = build_map(&m).expect("build");
        write_map(&cfg, &m).expect("write");
        for (name, bytes) in [
            ("raw_map.bin", a.raw_map),
            ("scripts.bin", a.scripts_bin),
            ("text.bin", a.text_bin),
        ] {
            let disk = std::fs::read(format!("{}/{}", cfg.output_dir, name)).expect("read file");
            assert_eq!(disk, bytes, "file {name} mismatch");
        }
        for (name, text) in [
            ("opcodes.hpp", a.opcodes_hpp),
            ("flags.hpp", a.flags_hpp),
            ("flag_bit_array.hpp", a.flag_bit_array_hpp),
            ("flag_bit_array.cpp", a.flag_bit_array_cpp),
            ("locations.hpp", a.locations_hpp),
            ("scripts.hpp", a.scripts_hpp),
        ] {
            let disk =
                std::fs::read_to_string(format!("{}/{}", cfg.output_dir, name)).expect("read hpp");
            assert_eq!(disk, text, "file {name} mismatch");
        }
    }

    #[test]
    fn build_map_parity_fixture_matches_go_reference() {
        // Byte-identical-to-Go fixture: hand-computed from Go mapbuilder.go.
        let mut m = empty_map();
        m.tiles[0][0] = 5;
        m.tiles[0][1] = 0x0102;
        m.tiles[3][10] = 0x03FF;
        m.npcs = vec![NPCObject {
            id: 10,
            x: 0,
            y: 0,
            sprite_id: 3,
            script: "msg {Hi}".to_string(),
        }];
        m.scripts = vec![
            ScriptObject {
                id: 20,
                x: 8,
                y: 4,
                script: "setflag flag_myflag".to_string(),
            },
            ScriptObject {
                id: 21,
                x: 240,
                y: 12,
                script: "msg {Bye}".to_string(),
            },
        ];
        let a = build_map(&m).expect("build");

        // raw_map.bin
        assert_eq!(a.raw_map.len(), 131072);
        assert_eq!(&a.raw_map[0..2], &[5, 0]);
        assert_eq!(&a.raw_map[2..4], &[0x02, 0x01]);
        assert_eq!(
            &a.raw_map[(3 * 256 + 10) * 2..(3 * 256 + 10) * 2 + 2],
            &[0xFF, 0x03]
        );
        assert_eq!(a.raw_map[4], 0);
        assert_eq!(&a.raw_map[131070..], &[0, 0]);

        // chunk 0 (0,0): msg {Hi} → [0,0,0] + 0xFF
        assert_eq!(&a.scripts_bin[0..4], &[0, 0, 0, 0xFF]);
        // chunk 33 (8,4): setflag flag_myflag → [6,0,0] + 0xFF
        let c33 = 33 * CHUNK_SIZE;
        assert_eq!(&a.scripts_bin[c33..c33 + 4], &[6, 0, 0, 0xFF]);
        // chunk (12/4=3)*32 + (240/8=30) = 126: msg {Bye} → [0,0,1] + 0xFF
        // (text "Bye" is the second deduplicated text, id 1)
        let c126 = 126 * CHUNK_SIZE;
        assert_eq!(&a.scripts_bin[c126..c126 + 4], &[0, 0, 1, 0xFF]);
        // untouched chunk stays zero
        let c100 = 100 * CHUNK_SIZE;
        assert_eq!(&a.scripts_bin[c100..c100 + 4], &[0, 0, 0, 0]);

        // text.bin: "Hi"=0, "Bye"=1 → count 2, offsets 0 and 4, then
        // LE length + bytes pairs.
        assert_eq!(
            a.text_bin,
            vec![2, 0, 0, 0, 4, 0, 2, 0, b'H', b'i', 3, 0, b'B', b'y', b'e']
        );

        // scripts.hpp mirrors chunk blobs (no End), sources as comments.
        assert_eq!(
            a.scripts_hpp,
            "#pragma once\n#include <stdint.h>\n// Auto-generated – DO NOT EDIT\n\n\
             // msg {Hi}\n\
             uint8_t blob0[] = { 0,0,0 };\n\
             // setflag flag_myflag\n\
             uint8_t blob33[] = { 6,0,0 };\n\
             // msg {Bye}\n\
             uint8_t blob126[] = { 0,0,1 };\n"
        );
    }
}
