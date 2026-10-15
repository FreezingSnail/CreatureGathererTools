//! Native duel-data generator (CreatureGathererFX-d73i.7).
//!
//! Additive, opt-in contract: `config.duel` names one authored TOML that folds
//! the authored 8-type roster to the duel type set and emits compact duel
//! records. Actual saved/preset kits stay authoritative; the generator only
//! folds move ids, species default types, explicit trainer persona/rate and
//! original-slot retypes, and named demo-team members. It never publishes
//! species-default option lists. No legacy record/layout/save changes.

use crate::builder::BuildError;
use crate::model::{Creature, Move, OpponentPartyMember, OpponentSeed};
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

const NO_TYPE: u8 = 255;
const EMPTY_MOVE: u8 = 255;
const RESERVED_MOVE_ID: i32 = 255;
const MAX_LIST_ITEMS: usize = 255;
const MAX_LEVEL: u8 = 31;
const MAX_RATE: u8 = 75;
const TEAM_SLOTS: usize = 3;

// Device-pinned five-type duel order and the fixed authored 8->5 fold. The
// generated chart/device assertions depend on this exact order and mapping, so
// the authored source may only restate it, never redefine it.
const CANON_ORDER: [&str; 5] = ["spirit", "water", "earth", "fire", "plant"];
const CANON_FOLD: [(&str, &str); 8] = [
    ("spirit", "spirit"),
    ("water", "water"),
    ("wind", "water"),
    ("earth", "earth"),
    ("fire", "fire"),
    ("lightning", "fire"),
    ("plant", "plant"),
    ("elder", "spirit"),
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    fold: Fold,
    personas: Personas,
    #[serde(default)]
    trainer: Vec<SourceTrainer>,
    team: Vec<SourceTeam>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fold {
    order: Vec<String>,
    map: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Personas {
    random: u8,
    reader: u8,
    frequency: u8,
    anti_repeat: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceTrainer {
    trainer_species: [String; 3],
    persona: String,
    rate: Option<u8>,
    #[serde(default)]
    member: Vec<SourceOverride>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceOverride {
    slot: u8,
    types: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceTeam {
    key: String,
    label: String,
    member: Vec<SourceMember>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMember {
    species: String,
    level: u8,
    #[serde(default)]
    types: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeciesTypes {
    pub types: [u8; 2],
    pub count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerEntry {
    pub persona: u8,
    pub rate: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotOverride {
    pub trainer: u8,
    pub slot: u8,
    pub types: [u8; 2],
    pub count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeamMember {
    pub species: u8,
    pub level: u8,
    pub types: [u8; 2],
    pub count: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Team {
    pub key: String,
    pub label: String,
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub order: Vec<String>,
    pub move_types: Vec<u8>,
    pub species: Vec<SpeciesTypes>,
    pub trainers: Vec<TrainerEntry>,
    pub overrides: Vec<SlotOverride>,
    pub teams: Vec<Team>,
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.as_bytes()[0].is_ascii_lowercase()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_key_label(
    key: &str,
    label: &str,
    kind: &str,
    keys: &mut HashSet<String>,
) -> Result<(), BuildError> {
    if !valid_key(key) || !keys.insert(key.to_owned()) {
        return Err(BuildError(format!(
            "{kind} has invalid or duplicate key {key:?}"
        )));
    }
    if !(1..=18).contains(&label.len())
        || !label.is_ascii()
        || !label.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        return Err(BuildError(format!(
            "{kind} {key:?} label must be 1..18 printable ASCII characters"
        )));
    }
    Ok(())
}

fn validate_list<T>(items: &[T], kind: &str) -> Result<(), BuildError> {
    if !(1..=MAX_LIST_ITEMS).contains(&items.len()) {
        return Err(BuildError(format!(
            "duel {kind} list must contain 1..={MAX_LIST_ITEMS} entries, found {}",
            items.len()
        )));
    }
    Ok(())
}

fn creature_id(name: &str, creatures: &[Creature], context: &str) -> Result<u8, BuildError> {
    let matches: Vec<_> = creatures
        .iter()
        .filter(|creature| creature.name.eq_ignore_ascii_case(name))
        .collect();
    if matches.len() != 1 {
        return Err(BuildError(format!(
            "{context}: species {name:?} resolves to {} canonical creatures",
            matches.len()
        )));
    }
    let id = u8::try_from(matches[0].id)
        .map_err(|_| BuildError(format!("{context}: species {name:?} has invalid ID")))?;
    if id == u8::MAX {
        return Err(BuildError(format!(
            "{context}: species {name:?} uses reserved empty ID 255"
        )));
    }
    Ok(id)
}

// Fold one authored type name to the duel set. "none"/"status" are ineligible
// (NO_TYPE); every other name must appear in the fold map and resolve to a
// declared duel type, which enforces fold completeness.
fn fold_name(
    order: &[String],
    map: &BTreeMap<String, String>,
    name: &str,
    context: &str,
) -> Result<u8, BuildError> {
    if name == "none" || name == "status" {
        return Ok(NO_TYPE);
    }
    let target = map
        .get(name)
        .ok_or_else(|| BuildError(format!("{context}: type {name:?} missing from fold map")))?;
    let index = order
        .iter()
        .position(|item| item == target)
        .ok_or_else(|| BuildError(format!("{context}: folded type {target:?} not in fold order")))?;
    Ok(u8::try_from(index).expect("fold order length checked"))
}

// Fold a member's authored type names and deduplicate after folding.
fn distinct_folded(
    order: &[String],
    map: &BTreeMap<String, String>,
    names: &[String],
    context: &str,
) -> Result<(u8, [u8; 2]), BuildError> {
    let mut types = [NO_TYPE; 2];
    let mut count = 0u8;
    for name in names {
        let folded = fold_name(order, map, name, context)?;
        if folded == NO_TYPE || types[..usize::from(count)].contains(&folded) {
            continue;
        }
        if count >= 2 {
            return Err(BuildError(format!(
                "{context}: more than two distinct folded types"
            )));
        }
        types[usize::from(count)] = folded;
        count += 1;
    }
    Ok((count, types))
}

fn persona_id(name: &str) -> Option<u8> {
    match name {
        "random" => Some(0),
        "reader" => Some(1),
        "frequency" => Some(2),
        "anti_repeat" => Some(3),
        _ => None,
    }
}

fn persona_rate(personas: &Personas, id: u8) -> u8 {
    match id {
        1 => personas.reader,
        2 => personas.frequency,
        3 => personas.anti_repeat,
        _ => personas.random,
    }
}

fn validate_opponent_member(
    member: OpponentPartyMember,
    moves: &[Move],
    move_types: &[u8],
    context: &str,
) -> Result<(), BuildError> {
    // A level-zero member is the empty-party sentinel and is always valid, so
    // one- and two-member trainer parties stay legal.
    if member.level == 0 {
        return Ok(());
    }
    if member.level > MAX_LEVEL {
        return Err(BuildError(format!(
            "{context}: opponent level {} outside 1..=31",
            member.level
        )));
    }
    let mut has_attack = false;
    // OpponentSeed packs the four move ids little-endian; 255 is the empty slot.
    for id in member.moves.to_le_bytes() {
        if id == EMPTY_MOVE {
            continue;
        }
        let matches = moves.iter().filter(|item| item.id == i32::from(id)).count();
        if matches != 1 {
            return Err(BuildError(format!(
                "{context}: packed opponent move ID {id} resolves to {matches} canonical moves"
            )));
        }
        if move_types
            .get(usize::from(id))
            .is_some_and(|type_| *type_ != NO_TYPE)
        {
            has_attack = true;
        }
    }
    if !has_attack {
        return Err(BuildError(format!(
            "{context}: opponent has no eligible authored attack"
        )));
    }
    Ok(())
}

pub fn parse(
    source: &str,
    creatures: &[Creature],
    moves: &[Move],
    opponents: &[OpponentSeed],
) -> Result<Catalog, BuildError> {
    let parsed: Source = toml::from_str(source)
        .map_err(|error| BuildError(format!("invalid duel TOML: {error}")))?;

    let order = parsed.fold.order;
    if order.len() != CANON_ORDER.len()
        || order
            .iter()
            .zip(CANON_ORDER.iter())
            .any(|(found, expected)| found != expected)
    {
        return Err(BuildError(format!(
            "duel fold.order must be exactly {CANON_ORDER:?}"
        )));
    }
    let map = parsed.fold.map;
    if map.len() != CANON_FOLD.len() {
        return Err(BuildError(format!(
            "duel fold.map must contain exactly the {} authored types, found {}",
            CANON_FOLD.len(),
            map.len()
        )));
    }
    for (from, to) in CANON_FOLD {
        match map.get(from) {
            Some(value) if value == to => {}
            Some(value) => {
                return Err(BuildError(format!(
                    "duel fold.map {from:?} must fold to {to:?}, found {value:?}"
                )))
            }
            None => {
                return Err(BuildError(format!(
                    "duel fold.map is missing authored type {from:?}"
                )))
            }
        }
    }

    let personas = parsed.personas;
    for (name, value) in [
        ("random", personas.random),
        ("reader", personas.reader),
        ("frequency", personas.frequency),
        ("anti_repeat", personas.anti_repeat),
    ] {
        if value >= MAX_RATE {
            return Err(BuildError(format!(
                "duel persona {name} rate {value} must be < {MAX_RATE}"
            )));
        }
    }

    // Per-move folded attack type, indexed by authored move id. Gaps and
    // utility/status/none slots stay NO_TYPE.
    let mut max_move_id = 0i32;
    for movement in moves {
        if movement.id == RESERVED_MOVE_ID {
            continue;
        }
        if movement.id < 0 {
            return Err(BuildError(format!("duel move id {} is negative", movement.id)));
        }
        if movement.id > max_move_id {
            max_move_id = movement.id;
        }
    }
    let move_count = usize::try_from(max_move_id + 1)
        .map_err(|_| BuildError("duel move id range too large".into()))?;
    if move_count > MAX_LIST_ITEMS {
        return Err(BuildError(format!(
            "duel move id range {move_count} exceeds {MAX_LIST_ITEMS}"
        )));
    }
    let mut move_types = vec![NO_TYPE; move_count];
    for movement in moves {
        if movement.id == RESERVED_MOVE_ID {
            continue;
        }
        let index = usize::try_from(movement.id)
            .map_err(|_| BuildError(format!("duel move id {} invalid", movement.id)))?;
        move_types[index] = fold_name(
            &order,
            &map,
            &movement.r#type,
            &format!("duel move {:?}", movement.name),
        )?;
    }

    // Every occupied authored opponent slot must hold at least one folded
    // eligible attack, whether or not the row carries an explicit persona or
    // retype. Empty (level-zero) slots are the sentinel and are skipped.
    for (row_index, row) in opponents.iter().enumerate() {
        for (slot, member) in [row.first, row.second, row.third].into_iter().enumerate() {
            validate_opponent_member(
                member,
                moves,
                &move_types,
                &format!("duel opponent row {row_index} member {}", slot + 1),
            )?;
        }
    }

    // Per-species folded, deduplicated default type pair, indexed by id.
    let mut max_species = 0i32;
    for creature in creatures {
        if creature.id < 0 {
            return Err(BuildError(format!(
                "duel creature id {} is negative",
                creature.id
            )));
        }
        if creature.id > max_species {
            max_species = creature.id;
        }
    }
    let species_count = usize::try_from(max_species + 1)
        .map_err(|_| BuildError("duel species id range too large".into()))?;
    if species_count > MAX_LIST_ITEMS {
        return Err(BuildError(format!(
            "duel species id range {species_count} exceeds {MAX_LIST_ITEMS}"
        )));
    }
    let mut species = vec![
        SpeciesTypes {
            types: [NO_TYPE; 2],
            count: 0,
        };
        species_count
    ];
    for creature in creatures {
        let context = format!("duel creature {:?}", creature.name);
        let (count, types) = distinct_folded(
            &order,
            &map,
            &[creature.type1.clone(), creature.type2.clone()],
            &context,
        )?;
        if count == 0 {
            return Err(BuildError(format!("{context}: no folded attack type")));
        }
        let index = usize::try_from(creature.id).expect("checked non-negative");
        species[index] = SpeciesTypes { types, count };
    }

    if opponents.len() > MAX_LIST_ITEMS {
        return Err(BuildError(format!(
            "duel opponent rows {} exceed {MAX_LIST_ITEMS}",
            opponents.len()
        )));
    }
    let mut trainers = vec![
        TrainerEntry {
            persona: 0,
            rate: personas.random,
        };
        opponents.len()
    ];
    let mut overrides = Vec::new();
    let mut seen_trainer = HashSet::new();
    for entry in parsed.trainer {
        let mut triple = [0u8; 3];
        for (index, name) in entry.trainer_species.iter().enumerate() {
            triple[index] =
                creature_id(name, creatures, &format!("duel trainer species {name:?}"))?;
        }
        let matching: Vec<_> = opponents
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                [
                    row.first.creature_id,
                    row.second.creature_id,
                    row.third.creature_id,
                ] == triple
            })
            .collect();
        if matching.len() != 1 {
            return Err(BuildError(format!(
                "duel trainer species triple {triple:?} resolves to {} opponent rows",
                matching.len()
            )));
        }
        let row = matching[0].0;
        if !seen_trainer.insert(row) {
            return Err(BuildError(format!(
                "duel trainer row {row} is listed more than once"
            )));
        }
        let persona = persona_id(&entry.persona).ok_or_else(|| {
            BuildError(format!(
                "duel trainer persona {:?} is not one of random/reader/frequency/anti_repeat",
                entry.persona
            ))
        })?;
        let rate = entry.rate.unwrap_or_else(|| persona_rate(&personas, persona));
        if rate >= MAX_RATE {
            return Err(BuildError(format!(
                "duel trainer rate {rate} must be < {MAX_RATE}"
            )));
        }
        let trainer = u8::try_from(row)
            .map_err(|_| BuildError("duel trainer index exceeds uint8_t".into()))?;
        trainers[row] = TrainerEntry { persona, rate };
        for override_entry in entry.member {
            if override_entry.slot > 2 {
                return Err(BuildError(format!(
                    "duel trainer row {row} retype slot {} outside 0..=2",
                    override_entry.slot
                )));
            }
            let occupied = match override_entry.slot {
                0 => matching[0].1.first.level != 0,
                1 => matching[0].1.second.level != 0,
                _ => matching[0].1.third.level != 0,
            };
            if !occupied {
                return Err(BuildError(format!(
                    "duel trainer row {row} retype slot {} is empty",
                    override_entry.slot
                )));
            }
            let context = format!("duel trainer row {row} slot {}", override_entry.slot);
            let (count, types) = distinct_folded(&order, &map, &override_entry.types, &context)?;
            if count == 0 {
                return Err(BuildError(format!(
                    "{context}: retype needs 1..=2 folded types"
                )));
            }
            overrides.push(SlotOverride {
                trainer,
                slot: override_entry.slot,
                types,
                count,
            });
        }
    }

    validate_list(&parsed.team, "team")?;
    let mut team_keys = HashSet::new();
    let mut teams = Vec::with_capacity(parsed.team.len());
    for team in parsed.team {
        validate_key_label(&team.key, &team.label, "duel team", &mut team_keys)?;
        if team.member.is_empty() || team.member.len() > 3 {
            return Err(BuildError(format!(
                "duel team {:?} needs 1..=3 members, found {}",
                team.key,
                team.member.len()
            )));
        }
        let mut members = Vec::with_capacity(team.member.len());
        for (index, member) in team.member.into_iter().enumerate() {
            let context = format!("duel team {:?} member {}", team.key, index + 1);
            if !(1..=MAX_LEVEL).contains(&member.level) {
                return Err(BuildError(format!(
                    "{context}: level {} outside 1..31",
                    member.level
                )));
            }
            let (count, types) = distinct_folded(&order, &map, &member.types, &context)?;
            members.push(TeamMember {
                species: creature_id(&member.species, creatures, &context)?,
                level: member.level,
                types,
                count,
            });
        }
        teams.push(Team {
            key: team.key,
            label: team.label,
            members,
        });
    }

    Ok(Catalog {
        order,
        move_types,
        species,
        trainers,
        overrides,
        teams,
    })
}

pub fn load(
    path: impl AsRef<Path>,
    creatures: &[Creature],
    moves: &[Move],
    opponents: &[OpponentSeed],
) -> Result<Catalog, BuildError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|error| BuildError(format!("reading {}: {error}", path.display())))?;
    parse(&source, creatures, moves, opponents)
        .map_err(|error| BuildError(format!("{}: {error}", path.display())))
}

fn write_byte_array(name: &mut String, symbol: &str, bytes: impl IntoIterator<Item = u8>) {
    let values: Vec<_> = bytes.into_iter().collect();
    let _ = write!(name, "uint8_t {symbol}[] = {{");
    for (index, byte) in values.iter().enumerate() {
        if index == 0 {
            name.push('\n');
        }
        if index % 16 == 0 {
            name.push_str("    ");
        }
        let _ = write!(name, "{byte}");
        if index + 1 < values.len() {
            name.push(',');
        }
        if index % 16 == 15 || index + 1 == values.len() {
            name.push('\n');
        } else {
            name.push(' ');
        }
    }
    name.push_str("};\n\n");
}

fn byte_block(symbol: &str, bytes: impl IntoIterator<Item = u8>) -> String {
    let mut values: Vec<u8> = bytes.into_iter().collect();
    // A declaration with an empty initializer breaks the expand parser; the
    // count constants stay authoritative, so pad an all-empty record with one
    // placeholder byte.
    if values.is_empty() {
        values.push(0);
    }
    let mut out = String::new();
    write_byte_array(&mut out, symbol, values);
    out
}

pub fn emit(
    catalog: &Catalog,
    output_dir: &Path,
    fixture_dir: Option<&Path>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(output_dir)
        .map_err(|error| anyhow::anyhow!("creating {}: {error}", output_dir.display()))?;

    let mut descriptor = String::from("namespace DuelData {\n\n");
    descriptor.push_str(&byte_block("duelMoveTypes", catalog.move_types.iter().copied()));
    descriptor.push_str(&byte_block(
        "duelSpeciesTypes",
        catalog
            .species
            .iter()
            .flat_map(|entry| [entry.count, entry.types[0], entry.types[1]]),
    ));
    descriptor.push_str(&byte_block(
        "duelTrainerPersona",
        catalog.trainers.iter().map(|entry| entry.persona),
    ));
    descriptor.push_str(&byte_block(
        "duelTrainerRate",
        catalog.trainers.iter().map(|entry| entry.rate),
    ));
    descriptor.push_str(&byte_block(
        "duelTrainerOverrides",
        catalog
            .overrides
            .iter()
            .flat_map(|entry| [entry.trainer, entry.slot, entry.count, entry.types[0], entry.types[1]]),
    ));
    descriptor.push_str(&byte_block(
        "duelTeamMemberCounts",
        catalog
            .teams
            .iter()
            .map(|team| u8::try_from(team.members.len()).unwrap_or(0)),
    ));
    // Fixed three slots per team, each five bytes, empty slots padded with a
    // level-zero sentinel: device indexing is team*15 + slot*5 with no offsets.
    descriptor.push_str(&byte_block(
        "duelTeamMembers",
        catalog.teams.iter().flat_map(|team| {
            let mut slots: Vec<u8> = Vec::with_capacity(TEAM_SLOTS * 5);
            for index in 0..TEAM_SLOTS {
                match team.members.get(index) {
                    Some(member) => slots.extend_from_slice(&[
                        member.species,
                        member.level,
                        member.count,
                        member.types[0],
                        member.types[1],
                    ]),
                    None => slots.extend_from_slice(&[0, 0, 0, NO_TYPE, NO_TYPE]),
                }
            }
            slots
        }),
    ));
    descriptor.push_str("\n} namespace_end\n");
    std::fs::write(output_dir.join("duel.txt"), descriptor)
        .map_err(|error| anyhow::anyhow!("writing duel.txt: {error}"))?;

    std::fs::write(output_dir.join("duel_ids.hpp"), ids_header(catalog))
        .map_err(|error| anyhow::anyhow!("writing duel_ids.hpp: {error}"))?;

    if let Some(fixture_dir) = fixture_dir {
        std::fs::create_dir_all(fixture_dir)
            .map_err(|error| anyhow::anyhow!("creating {}: {error}", fixture_dir.display()))?;
        std::fs::write(fixture_dir.join("duel_data.hpp"), fixture_header(catalog))
            .map_err(|error| anyhow::anyhow!("writing duel_data.hpp: {error}"))?;
    }
    Ok(())
}

fn ids_header(catalog: &Catalog) -> String {
    let mut output =
        String::from("#pragma once\n#include <stdint.h>\n\nnamespace DuelIds {\n");
    let _ = writeln!(
        output,
        "constexpr uint8_t typeCount = {};",
        catalog.order.len()
    );
    let _ = writeln!(output, "constexpr uint8_t noType = {};", NO_TYPE);
    let _ = writeln!(
        output,
        "constexpr uint8_t moveCount = {};",
        catalog.move_types.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t speciesCount = {};",
        catalog.species.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t trainerCount = {};",
        catalog.trainers.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t trainerOverrideCount = {};",
        catalog.overrides.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t teamCount = {};",
        catalog.teams.len()
    );
    output.push_str("constexpr uint8_t speciesBytes = 3;\nconstexpr uint8_t trainerOverrideBytes = 5;\nconstexpr uint8_t teamSlots = 3;\nconstexpr uint8_t teamMemberBytes = 5;\n");
    for (index, team) in catalog.teams.iter().enumerate() {
        let _ = writeln!(output, "constexpr uint8_t team_{} = {index};", team.key);
    }
    output.push_str("} // namespace DuelIds\n");
    output
}

fn fixture_header(catalog: &Catalog) -> String {
    let mut output =
        String::from("#pragma once\n#include <stdint.h>\n\nnamespace DuelFixture {\n");
    let _ = writeln!(
        output,
        "constexpr uint8_t typeCount = {};",
        catalog.order.len()
    );
    let _ = writeln!(output, "constexpr uint8_t noType = {};", NO_TYPE);
    let _ = writeln!(
        output,
        "constexpr uint8_t moveCount = {};",
        catalog.move_types.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t speciesCount = {};",
        catalog.species.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t trainerCount = {};",
        catalog.trainers.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t trainerOverrideCount = {};",
        catalog.overrides.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t teamCount = {};",
        catalog.teams.len()
    );
    let values = |mut bytes: Vec<u8>| {
        if bytes.is_empty() {
            bytes.push(0);
        }
        bytes
            .iter()
            .map(u8::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let _ = writeln!(
        output,
        "const uint8_t duelMoveTypes[] = {{ {} }};",
        values(catalog.move_types.clone())
    );
    let _ = writeln!(
        output,
        "const uint8_t duelSpeciesTypes[] = {{ {} }};",
        values(
            catalog
                .species
                .iter()
                .flat_map(|entry| [entry.count, entry.types[0], entry.types[1]])
                .collect()
        )
    );
    let _ = writeln!(
        output,
        "const uint8_t duelTrainerPersona[] = {{ {} }};",
        values(catalog.trainers.iter().map(|entry| entry.persona).collect())
    );
    let _ = writeln!(
        output,
        "const uint8_t duelTrainerRate[] = {{ {} }};",
        values(catalog.trainers.iter().map(|entry| entry.rate).collect())
    );
    let _ = writeln!(
        output,
        "const uint8_t duelTrainerOverrides[] = {{ {} }};",
        values(
            catalog
                .overrides
                .iter()
                .flat_map(|entry| [entry.trainer, entry.slot, entry.count, entry.types[0], entry.types[1]])
                .collect()
        )
    );
    let _ = writeln!(
        output,
        "const uint8_t duelTeamMemberCounts[] = {{ {} }};",
        values(
            catalog
                .teams
                .iter()
                .map(|team| u8::try_from(team.members.len()).unwrap_or(0))
                .collect()
        )
    );
    let _ = writeln!(
        output,
        "const uint8_t duelTeamMembers[] = {{ {} }};",
        values(
            catalog
                .teams
                .iter()
                .flat_map(|team| {
                    let mut slots: Vec<u8> = Vec::with_capacity(TEAM_SLOTS * 5);
                    for index in 0..TEAM_SLOTS {
                        match team.members.get(index) {
                            Some(member) => slots.extend_from_slice(&[
                                member.species,
                                member.level,
                                member.count,
                                member.types[0],
                                member.types[1],
                            ]),
                            None => slots.extend_from_slice(&[0, 0, 0, NO_TYPE, NO_TYPE]),
                        }
                    }
                    slots
                })
                .collect()
        )
    );
    let _ = writeln!(output, "constexpr uint8_t teamSlots = {TEAM_SLOTS};");
    for (index, team) in catalog.teams.iter().enumerate() {
        let _ = writeln!(output, "constexpr uint8_t team_{} = {index};", team.key);
    }
    output.push_str("} // namespace DuelFixture\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = r#"
[fold]
order = ["spirit", "water", "earth", "fire", "plant"]
[fold.map]
spirit = "spirit"
water = "water"
wind = "water"
earth = "earth"
fire = "fire"
lightning = "fire"
plant = "plant"
elder = "spirit"

[personas]
random = 0
reader = 25
frequency = 50
anti_repeat = 50

[[trainer]]
trainer_species = ["SquibbleSnail", "SkitterCrab", "squid"]
persona = "reader"
[[trainer.member]]
slot = 0
types = ["earth"]

[[team]]
key = "balanced"
label = "Balanced"
[[team.member]]
species = "squid"
level = 31
types = ["water"]
"#;

    fn creature(name: &str, id: i32, type1: &str, type2: &str) -> Creature {
        Creature {
            name: name.into(),
            id,
            type1: type1.into(),
            type2: type2.into(),
            evo_level: 0,
            atk: 1,
            def: 1,
            spc_atk: 1,
            spc_def: 1,
            hp: 1,
            spd: 1,
            move1: 0,
            move2: 0,
            move3: 0,
            move4: 0,
            move_list: vec![],
        }
    }

    fn movement(name: &str, id: i32, r#type: &str) -> Move {
        Move {
            id,
            name: name.into(),
            r#type: r#type.into(),
            power: 1,
            physical: 0,
            accuracy: 100,
            effect: "NONE".into(),
        }
    }

    fn models() -> (Vec<Creature>, Vec<Move>, Vec<OpponentSeed>) {
        let creatures = vec![
            creature("SquibbleSnail", 0, "wind", "none"),
            creature("SkitterCrab", 3, "earth", "none"),
            creature("squid", 6, "water", "none"),
            creature("OrbitRelic", 7, "elder", "spirit"),
        ];
        let moves = vec![
            movement("smite", 0, "spirit"),
            movement("splash", 4, "water"),
            movement("breeze", 8, "wind"),
            movement("bolt", 20, "lightning"),
            movement("Sharpen", 37, "status"),
            movement("none", 255, "none"),
        ];
        let member = |id| OpponentPartyMember {
            creature_id: id,
            level: 31,
            moves: u32::from_le_bytes([0, 255, 255, 255]),
        };
        let opponents = vec![OpponentSeed {
            first: member(0),
            second: member(3),
            third: member(6),
        }];
        (creatures, moves, opponents)
    }

    fn reject(source: &str, expected: &str) {
        let (creatures, moves, opponents) = models();
        let error = parse(source, &creatures, &moves, &opponents).unwrap_err();
        assert!(error.0.contains(expected), "{error}");
    }

    #[test]
    fn resolves_fold_moves_species_personas_retypes_and_teams() {
        let (creatures, moves, opponents) = models();
        let catalog = parse(VALID, &creatures, &moves, &opponents).unwrap();
        assert_eq!(catalog.order.len(), 5);
        // Move id order: spirit->0, water->1, wind->1 (water), status->255.
        assert_eq!(catalog.move_types.len(), 38);
        assert_eq!(catalog.move_types[0], 0);
        assert_eq!(catalog.move_types[4], 1);
        assert_eq!(catalog.move_types[8], 1);
        assert_eq!(catalog.move_types[20], 3);
        assert_eq!(catalog.move_types[37], NO_TYPE);
        // Species default folded types (wind->water, elder+spirit dedup to 1).
        assert_eq!(catalog.species[0].count, 1);
        assert_eq!(catalog.species[0].types[0], 1);
        assert_eq!(catalog.species[3].types[0], 2);
        assert_eq!(catalog.species[7].count, 1);
        assert_eq!(catalog.species[7].types[0], 0);
        // Explicit trainer persona/rate and original-slot retype.
        assert_eq!(catalog.trainers[0].persona, 1);
        assert_eq!(catalog.trainers[0].rate, 25);
        assert_eq!(catalog.overrides.len(), 1);
        assert_eq!(catalog.overrides[0].trainer, 0);
        assert_eq!(catalog.overrides[0].slot, 0);
        assert_eq!(catalog.overrides[0].count, 1);
        assert_eq!(catalog.overrides[0].types[0], 2);
        // Named demo team member with authored level and retype.
        assert_eq!(catalog.teams.len(), 1);
        assert_eq!(catalog.teams[0].members[0].species, 6);
        assert_eq!(catalog.teams[0].members[0].level, 31);
        assert_eq!(catalog.teams[0].members[0].types[0], 1);
    }

    #[test]
    fn dedupes_secondary_folded_type_and_allows_missing_retype() {
        let (creatures, moves, opponents) = models();
        let catalog = parse(VALID, &creatures, &moves, &opponents).unwrap();
        // wind+water collapse, so SquibbleSnail keeps one type; the team member
        // has an explicit retype and the species default is never an option list.
        assert_eq!(catalog.species[0].types, [1, NO_TYPE]);
        assert_eq!(catalog.teams[0].members[0].count, 1);
    }

    #[test]
    fn defaults_unlisted_trainers_to_random_zero() {
        let (creatures, moves, opponents) = models();
        let source = VALID.replace(
            "[[trainer]]\ntrainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]\npersona = \"reader\"\n[[trainer.member]]\nslot = 0\ntypes = [\"earth\"]\n",
            "",
        );
        let catalog = parse(&source, &creatures, &moves, &opponents).unwrap();
        assert_eq!(catalog.trainers[0].persona, 0);
        assert_eq!(catalog.trainers[0].rate, 0);
        assert!(catalog.overrides.is_empty());
    }

    #[test]
    fn rejects_fold_persona_retype_team_and_trainer_bounds() {
        reject(
            &VALID.replace("wind = \"water\"\n", ""),
            "must contain exactly",
        );
        reject(
            &VALID.replace("order = [\"spirit\", \"water\", \"earth\", \"fire\", \"plant\"]",
                "order = [\"water\", \"spirit\", \"earth\", \"fire\", \"plant\"]"),
            "must be exactly",
        );
        reject(
            &VALID.replace("wind = \"water\"", "wind = \"earth\""),
            "must fold to",
        );
        reject(
            &VALID.replace("reader = 25", "reader = 75"),
            "must be < 75",
        );
        reject(
            &VALID.replace("persona = \"reader\"", "persona = \"boss\""),
            "is not one of",
        );
        reject(
            &VALID.replace("persona = \"reader\"\n", "persona = \"reader\"\nrate = 90\n"),
            "must be < 75",
        );
        reject(
            &VALID.replace("slot = 0\n", "slot = 3\n"),
            "outside 0..=2",
        );
        reject(
            &VALID.replace("types = [\"earth\"]", "types = []"),
            "retype needs 1..=2",
        );
        reject(
            &VALID.replace("types = [\"earth\"]", "types = [\"unknown\"]"),
            "missing from fold map",
        );
        reject(
            &VALID.replace("level = 31", "level = 0"),
            "outside 1..31",
        );
        reject(
            &VALID.replace("level = 31", "level = 32"),
            "outside 1..31",
        );
        reject(
            &VALID.replace("key = \"balanced\"", "key = \"Bad-Key\""),
            "invalid or duplicate key",
        );
        reject(
            &VALID.replace("species = \"squid\"", "species = \"missing\""),
            "resolves to 0 canonical creatures",
        );
    }

    #[test]
    fn rejects_unknown_or_duplicate_trainer_triples_and_empty_opponent_kits() {
        reject(
            &VALID.replace(
                "trainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]",
                "trainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"SquibbleSnail\"]",
            ),
            "0 opponent rows",
        );
        reject(
            &format!(
                "{VALID}\n[[trainer]]\ntrainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]\npersona = \"frequency\"\n"
            ),
            "listed more than once",
        );
        let (creatures, moves, mut opponents) = models();
        opponents[0].first.moves = u32::MAX;
        let error = parse(VALID, &creatures, &moves, &opponents).unwrap_err();
        assert!(error.0.contains("no eligible authored attack"), "{error}");
        let (creatures, moves, mut opponents) = models();
        opponents[0].first.level = 32;
        let error = parse(VALID, &creatures, &moves, &opponents).unwrap_err();
        assert!(error.0.contains("outside 1..=31"), "{error}");
    }

    #[test]
    fn rejects_utility_only_opponent_kit() {
        let (creatures, moves, mut opponents) = models();
        // Sharpen (id 37) folds to NO_TYPE, so a status-only kit must fail even
        // though every packed id resolves to a canonical move.
        opponents[0].first.moves = u32::from_le_bytes([37, 255, 255, 255]);
        let error = parse(VALID, &creatures, &moves, &opponents).unwrap_err();
        assert!(error.0.contains("no eligible authored attack"), "{error}");
    }

    #[test]
    fn accepts_partial_opponent_party_with_empty_sentinel_slots() {
        let (creatures, moves, mut opponents) = models();
        opponents[0].second.level = 0;
        opponents[0].third.level = 0;
        let catalog = parse(VALID, &creatures, &moves, &opponents).unwrap();
        assert_eq!(catalog.trainers[0].persona, 1);
    }

    #[test]
    fn rejects_retype_of_empty_opponent_slot() {
        let (creatures, moves, mut opponents) = models();
        opponents[0].second.level = 0;
        let source = VALID.replace(
            "persona = \"reader\"\n",
            "persona = \"reader\"\n[[trainer.member]]\nslot = 1\ntypes = [\"earth\"]\n",
        );
        let error = parse(&source, &creatures, &moves, &opponents).unwrap_err();
        assert!(error.0.contains("retype slot 1 is empty"), "{error}");
    }

    #[test]
    fn accepts_species_zero_custom_move_ids_and_default_kits() {
        let (creatures, moves, opponents) = models();
        assert_eq!(creatures[0].id, 0, "species zero stays a valid id");
        let source = VALID.replace(
            "species = \"squid\"\nlevel = 31\ntypes = [\"water\"]",
            "species = \"SquibbleSnail\"\nlevel = 3",
        );
        let catalog = parse(&source, &creatures, &moves, &opponents).unwrap();
        let member = catalog.teams[0].members[0];
        assert_eq!(member.species, 0);
        assert_eq!(member.level, 3);
        assert_eq!(member.count, 0, "no retype falls back to species types");
        // Folded per authored move id, independent of any species default kit.
        assert_eq!(catalog.move_types[20], 3);
    }

    #[test]
    fn load_prefixes_path_and_emits_compact_records() {
        let (creatures, moves, opponents) = models();
        let error = load("missing-duel.toml", &creatures, &moves, &opponents).unwrap_err();
        assert!(error.0.contains("missing-duel.toml"), "{error}");

        let catalog = parse(VALID, &creatures, &moves, &opponents).unwrap();
        let ids = ids_header(&catalog);
        assert!(ids.contains("constexpr uint8_t typeCount = 5;"));
        assert!(ids.contains("constexpr uint8_t team_balanced = 0;"));
        assert!(ids.contains("constexpr uint8_t teamSlots = 3;"));
        assert!(ids.contains("constexpr uint8_t teamMemberBytes = 5;"));
        let fixture = fixture_header(&catalog);
        assert!(fixture.contains("const uint8_t duelMoveTypes[]"));
        assert!(fixture.contains("const uint8_t duelTeamMembers[]"));
        assert!(fixture.contains("constexpr uint8_t teamSlots = 3;"));
    }
}
