//! Strict parser for the authored arena demo roster.

use crate::builder::BuildError;
use crate::model::{attack_mask, Creature, OpponentPartyMember, OpponentSeed};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::fxpack::font::{self, Atlas};

const MAX_LIST_ITEMS: usize = 255;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    label_font: String,
    player_team: Vec<SourcePlayer>,
    opponent_team: Vec<SourceOpponent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePlayer {
    key: String,
    label: String,
    members: Vec<SourceMember>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMember {
    species: String,
    level: u8,
    moves: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceOpponent {
    key: String,
    label: String,
    trainer_species: [String; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub species: u8,
    pub level: u8,
    pub attack_mask: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerTeam {
    pub key: String,
    pub label: String,
    pub members: [Member; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpponentTeam {
    pub key: String,
    pub label: String,
    pub trainer_id: u8,
    pub species: [u8; 3],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Catalog {
    pub label_font: PathBuf,
    pub players: Vec<PlayerTeam>,
    pub opponents: Vec<OpponentTeam>,
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.as_bytes()[0].is_ascii_lowercase()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn validate_list<T>(items: &[T], kind: &str) -> Result<(), BuildError> {
    if !(1..=MAX_LIST_ITEMS).contains(&items.len()) {
        return Err(BuildError(format!(
            "arena {kind} list must contain 1..={MAX_LIST_ITEMS} entries, found {}",
            items.len()
        )));
    }
    Ok(())
}

fn validate_key_label(
    key: &str,
    label: &str,
    kind: &str,
    keys: &mut HashSet<String>,
) -> Result<(), BuildError> {
    if !valid_key(key) || !keys.insert(key.to_owned()) {
        return Err(BuildError(format!(
            "arena {kind} has invalid or duplicate key {key:?}"
        )));
    }
    if !(1..=18).contains(&label.len())
        || !label.is_ascii()
        || !label.bytes().all(|b| (0x20..=0x7e).contains(&b))
    {
        return Err(BuildError(format!(
            "arena {kind} {key:?} label must be 1..18 printable ASCII characters"
        )));
    }
    Ok(())
}

fn species_id(name: &str, creatures: &[Creature], context: &str) -> Result<u8, BuildError> {
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

fn validate_opponent_member(
    member: OpponentPartyMember,
    context: &str,
) -> Result<(), BuildError> {
    if !(1..=31).contains(&member.level) {
        return Err(BuildError(format!(
            "{context}: opponent level {} outside 1..31",
            member.level
        )));
    }
    if member.attack_mask & 0x1F == 0 {
        return Err(BuildError(format!(
            "{context}: opponent has no usable authored move"
        )));
    }
    Ok(())
}

pub fn parse(
    source: &str,
    creatures: &[Creature],
    opponents: &[OpponentSeed],
) -> Result<Catalog, BuildError> {
    let parsed: Source = toml::from_str(source)
        .map_err(|error| BuildError(format!("invalid arena demo TOML: {error}")))?;
    validate_list(&parsed.player_team, "player_team")?;
    validate_list(&parsed.opponent_team, "opponent_team")?;
    if parsed.label_font.is_empty() {
        return Err(BuildError("arena label_font must not be empty".into()));
    }

    let mut player_keys = HashSet::new();
    let mut players = Vec::with_capacity(parsed.player_team.len());
    for team in parsed.player_team {
        validate_key_label(&team.key, &team.label, "player team", &mut player_keys)?;
        if team.members.len() != 3 {
            return Err(BuildError(format!(
                "player team {:?} needs exactly 3 members, found {}",
                team.key,
                team.members.len()
            )));
        }
        let mut resolved = Vec::with_capacity(3);
        for (index, member) in team.members.into_iter().enumerate() {
            let context = format!("player team {:?} member {}", team.key, index + 1);
            if !(1..=31).contains(&member.level) {
                return Err(BuildError(format!(
                    "{context}: level {} outside 1..31",
                    member.level
                )));
            }
            let folded = attack_mask(&member.moves);
            if folded == 0 {
                return Err(BuildError(format!(
                    "{context}: member has no authored move"
                )));
            }
            resolved.push(Member {
                species: species_id(&member.species, creatures, &context)?,
                level: member.level,
                attack_mask: folded,
            });
        }
        let members: [Member; 3] = resolved.try_into().expect("length checked");
        players.push(PlayerTeam {
            key: team.key,
            label: team.label,
            members,
        });
    }

    let mut opponent_keys = HashSet::new();
    let mut resolved_opponents = Vec::with_capacity(parsed.opponent_team.len());
    for team in parsed.opponent_team {
        validate_key_label(&team.key, &team.label, "opponent team", &mut opponent_keys)?;
        let mut species = [0; 3];
        for (index, name) in team.trainer_species.iter().enumerate() {
            species[index] = species_id(
                name,
                creatures,
                &format!("opponent team {:?} member {}", team.key, index + 1),
            )?;
        }
        let matching: Vec<_> = opponents
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                [
                    row.first.creature_id,
                    row.second.creature_id,
                    row.third.creature_id,
                ] == species
            })
            .collect();
        if matching.len() != 1 {
            return Err(BuildError(format!(
                "opponent team {:?} species triple resolves to {} canonical opponent rows",
                team.key,
                matching.len()
            )));
        }
        let (trainer_index, seed) = matching[0];
        let trainer_id = u8::try_from(trainer_index).map_err(|_| {
            BuildError(format!(
                "opponent team {:?} trainer index exceeds uint8_t",
                team.key
            ))
        })?;
        if trainer_id == u8::MAX {
            return Err(BuildError(format!(
                "opponent team {:?} trainer index 255 is reserved",
                team.key
            )));
        }
        for (index, member) in [seed.first, seed.second, seed.third]
            .into_iter()
            .enumerate()
        {
            validate_opponent_member(
                member,
                &format!("opponent team {:?} member {}", team.key, index + 1),
            )?;
        }
        resolved_opponents.push(OpponentTeam {
            key: team.key,
            label: team.label,
            trainer_id,
            species,
        });
    }

    Ok(Catalog {
        label_font: PathBuf::from(parsed.label_font),
        players,
        opponents: resolved_opponents,
    })
}

pub fn load(
    path: impl AsRef<Path>,
    creatures: &[Creature],
    opponents: &[OpponentSeed],
) -> Result<Catalog, BuildError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|error| BuildError(format!("reading {}: {error}", path.display())))?;
    let mut catalog = parse(&source, creatures, opponents)
        .map_err(|error| BuildError(format!("{}: {error}", path.display())))?;
    let font = PathBuf::from(catalog.label_font);
    catalog.label_font = if font.is_absolute() {
        font
    } else {
        path.parent().unwrap_or_else(|| Path::new(".")).join(font)
    };
    Ok(catalog)
}

/// Emits the production arena descriptor, symbolic IDs, and optional fixture
/// expectations from one already validated catalog.
pub fn emit(
    catalog: &Catalog,
    output_dir: &Path,
    fixture_dir: Option<&Path>,
) -> anyhow::Result<()> {
    let atlas = Atlas::load(&catalog.label_font, 5, 6, 0).map_err(|error| {
        anyhow::anyhow!(
            "loading arena label font {}: {error}",
            catalog.label_font.display()
        )
    })?;
    let mut labels = Vec::with_capacity(catalog.players.len() + catalog.opponents.len());
    let label_sources = catalog
        .players
        .iter()
        .map(|team| (format!("player_{}", team.key), team.label.as_str()))
        .chain(
            catalog
                .opponents
                .iter()
                .map(|team| (format!("opponent_{}", team.key), team.label.as_str())),
        );
    for (symbol, text) in label_sources {
        let rendered = font::render_strings(&atlas, &[(symbol.clone(), text.to_owned())], 2)
            .map_err(|error| anyhow::anyhow!("rendering arena label {symbol}: {error}"))?
            .pop()
            .expect("one label produces one image");
        let sheet = rendered.sheet;
        if sheet.height != 8 || sheet.width > 95 || sheet.frames != 2 {
            anyhow::bail!(
                "arena label {symbol} has unsupported geometry {}x{}, {} frames (expected height 8, width <=95, 2 frames)",
                sheet.width,
                sheet.height,
                sheet.frames
            );
        }
        labels.push((symbol, sheet));
    }

    std::fs::create_dir_all(output_dir)
        .map_err(|error| anyhow::anyhow!("creating {}: {error}", output_dir.display()))?;
    let mut descriptor = String::from("namespace ArenaDemoData {\n\n");
    let members = catalog
        .players
        .iter()
        .flat_map(|team| team.members)
        .flat_map(|member| [member.species, member.level, member.attack_mask]);
    write_byte_array(&mut descriptor, "playerMembers", members);
    write_byte_array(
        &mut descriptor,
        "opponentTrainerIds",
        catalog.opponents.iter().map(|team| team.trainer_id),
    );
    write_byte_array(
        &mut descriptor,
        "opponentSpecies",
        catalog.opponents.iter().flat_map(|team| team.species),
    );
    for (symbol, sheet) in &labels {
        write_byte_array(&mut descriptor, symbol, sheet.bytes.iter().copied());
    }
    write_pointer_table(
        &mut descriptor,
        "playerLabels",
        catalog
            .players
            .iter()
            .map(|team| format!("player_{}", team.key)),
    );
    write_pointer_table(
        &mut descriptor,
        "opponentLabels",
        catalog
            .opponents
            .iter()
            .map(|team| format!("opponent_{}", team.key)),
    );
    write_byte_array(
        &mut descriptor,
        "playerLabelWidths",
        catalog
            .players
            .iter()
            .map(|team| label_width(&labels, "player", &team.key)),
    );
    write_byte_array(
        &mut descriptor,
        "opponentLabelWidths",
        catalog
            .opponents
            .iter()
            .map(|team| label_width(&labels, "opponent", &team.key)),
    );
    descriptor.push_str("\n} namespace_end\n");
    std::fs::write(output_dir.join("arena_demo.txt"), descriptor)
        .map_err(|error| anyhow::anyhow!("writing arena_demo.txt: {error}"))?;
    std::fs::write(output_dir.join("arena_demo_ids.hpp"), ids_header(catalog))
        .map_err(|error| anyhow::anyhow!("writing arena_demo_ids.hpp: {error}"))?;
    if let Some(fixture_dir) = fixture_dir {
        std::fs::create_dir_all(fixture_dir)
            .map_err(|error| anyhow::anyhow!("creating {}: {error}", fixture_dir.display()))?;
        std::fs::write(
            fixture_dir.join("arena_demo_data.hpp"),
            fixture_header(catalog),
        )
        .map_err(|error| anyhow::anyhow!("writing arena_demo_data.hpp: {error}"))?;
    }
    Ok(())
}

fn label_width(labels: &[(String, crate::fxpack::sprite::Sheet)], kind: &str, key: &str) -> u8 {
    labels
        .iter()
        .find(|(name, _)| name == &format!("{kind}_{key}"))
        .and_then(|(_, sheet)| u8::try_from(sheet.width).ok())
        .expect("label width was validated")
}

fn write_byte_array(name: &mut String, symbol: &str, bytes: impl IntoIterator<Item = u8>) {
    use std::fmt::Write as _;
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

fn write_pointer_table(
    name: &mut String,
    symbol: &str,
    pointers: impl IntoIterator<Item = String>,
) {
    use std::fmt::Write as _;
    let values: Vec<_> = pointers.into_iter().collect();
    let _ = writeln!(name, "uint24_t {symbol}[] = {{ {} }};\n", values.join(", "));
}

fn ids_header(catalog: &Catalog) -> String {
    use std::fmt::Write as _;
    let mut output =
        String::from("#pragma once\n#include <stdint.h>\n\nnamespace ArenaDemoIds {\n");
    let _ = writeln!(
        output,
        "constexpr uint8_t playerCount = {};",
        catalog.players.len()
    );
    let _ = writeln!(
        output,
        "constexpr uint8_t opponentCount = {};",
        catalog.opponents.len()
    );
    output.push_str("constexpr uint8_t MemberBytes = 3;\nconstexpr uint8_t PlayerBytes = 9;\nconstexpr uint8_t OpponentSpeciesBytes = 3;\n");
    for (index, team) in catalog.players.iter().enumerate() {
        let _ = writeln!(output, "constexpr uint8_t player_{} = {index};", team.key);
    }
    for (index, team) in catalog.opponents.iter().enumerate() {
        let _ = writeln!(output, "constexpr uint8_t opponent_{} = {index};", team.key);
        let _ = writeln!(
            output,
            "constexpr uint8_t trainer_{} = {};",
            team.key, team.trainer_id
        );
    }
    output.push_str("} // namespace ArenaDemoIds\n");
    output
}

fn fixture_header(catalog: &Catalog) -> String {
    use std::fmt::Write as _;
    let mut output =
        String::from("#pragma once\n#include <stdint.h>\n\nnamespace ArenaDemoFixture {\n");
    for team in &catalog.players {
        let values = team
            .members
            .iter()
            .flat_map(|member| [member.species, member.level, member.attack_mask])
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let _ = writeln!(
            output,
            "const uint8_t player_{}[] = {{ {} }};",
            team.key,
            values.join(", ")
        );
    }
    for team in &catalog.opponents {
        let species = team.species.iter().map(u8::to_string).collect::<Vec<_>>();
        let _ = writeln!(
            output,
            "constexpr uint8_t trainer_{} = {};",
            team.key, team.trainer_id
        );
        let _ = writeln!(
            output,
            "const uint8_t opponent_{}[] = {{ {} }};",
            team.key,
            species.join(", ")
        );
    }
    output.push_str("} // namespace ArenaDemoFixture\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OpponentPartyMember;

    const VALID: &str = include_str!("../testdata/arena_demo/arena-demo.toml");

    fn creature(name: &str, id: i32) -> Creature {
        Creature {
            name: name.into(),
            id,
            type1: "wind".into(),
            type2: "none".into(),
            evo_level: 0,
            hp: 1,
            evolves_to: None,
            moves: vec![],
        }
    }

    fn models() -> (Vec<Creature>, Vec<OpponentSeed>) {
        let creatures = vec![
            creature("SquibbleSnail", 0),
            creature("SkitterCrab", 3),
            creature("squid", 6),
        ];
        let member = |id| OpponentPartyMember {
            creature_id: id,
            level: 31,
            attack_mask: 0b00010, // water
        };
        let opponents = vec![OpponentSeed {
            first: member(0),
            second: member(3),
            third: member(6),
        }];
        (creatures, opponents)
    }

    fn reject(source: &str, expected: &str) {
        let (creatures, opponents) = models();
        let error = parse(source, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains(expected), "{error}");
    }

    #[test]
    fn resolves_valid_roster_and_sentinels() {
        let (creatures, opponents) = models();
        let catalog = parse(VALID, &creatures, &opponents).unwrap();
        assert_eq!(catalog.players.len(), 1);
        assert_eq!(catalog.players[0].members[0].species, 0);
        assert_eq!(catalog.players[0].members[0].attack_mask, 0b00010);
        assert_eq!(catalog.opponents[0].trainer_id, 0);
        assert_eq!(catalog.opponents[0].species, [0, 3, 6]);
        assert!(catalog.label_font.ends_with("../images/ArduFont_5x6.png"));
    }

    #[test]
    fn species_are_case_insensitive_and_unknown_types_reject() {
        let (creatures, opponents) = models();
        let parsed = parse(
            &VALID.replace("SquibbleSnail", "sQuIbBlEsNaIl"),
            &creatures,
            &opponents,
        )
        .unwrap();
        assert_eq!(parsed.players[0].members[0].species, 0);
        reject(
            &VALID.replace("\"water\"", "\"status\""),
            "no authored move",
        );
    }

    #[test]
    fn rejects_unknown_fields_missing_values_and_invalid_members() {
        reject(&format!("{VALID}\nunknown = 1\n"), "unknown field");
        reject(
            &VALID.replace("label_font = \"../images/ArduFont_5x6.png\"\n", ""),
            "missing field",
        );
        reject(&VALID.replace("level = 31", "level = 0"), "outside 1..31");
        reject(&VALID.replace("level = 31", "level = 32"), "outside 1..31");
        reject(
            &VALID.replace(
                "moves = [\"water\", \"none\", \"none\", \"none\"]",
                "moves = [\"none\", \"none\", \"none\", \"none\"]",
            ),
            "no authored move",
        );
        reject(
            &VALID.replace(
                "[[player_team.members]]\nspecies = \"squid\"\nlevel = 31\nmoves = [\"water\", \"none\", \"none\", \"none\"]\n",
                "",
            ),
            "exactly 3 members",
        );
    }

    #[test]
    fn rejects_keys_labels_species_and_trainer_rows() {
        reject(
            &VALID.replace("key = \"starter\"", "key = \"Bad-Key\""),
            "invalid or duplicate key",
        );
        reject(
            &VALID.replace("label = \"Starter\"", "label = \"\""),
            "printable ASCII",
        );
        reject(
            &VALID.replace("label = \"Starter\"", "label = \"This label is too long\""),
            "printable ASCII",
        );
        reject(
            &VALID.replace("SquibbleSnail", "missingSpecies"),
            "resolves to 0 canonical creatures",
        );
        reject(
            &VALID.replace(
                "trainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]",
                "trainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"SquibbleSnail\"]",
            ),
            "0 canonical opponent rows",
        );
        reject(
            &format!(
                "{VALID}\n[[opponent_team]]\nkey = \"rival\"\nlabel = \"Duplicate\"\ntrainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]\n"
            ),
            "duplicate key",
        );
    }

    #[test]
    fn rejects_ambiguous_ids_empty_opponent_masks_and_reserved_trainer_index() {
        let (mut creatures, opponents) = models();
        creatures.push(creature("sQUIBBLESNAIL", 1));
        let error = parse(VALID, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("resolves to 2 canonical creatures"));

        let (creatures, mut opponents) = models();
        opponents.push(opponents[0]);
        let error = parse(VALID, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("resolves to 2 canonical opponent rows"));

        let (creatures, mut opponents) = models();
        opponents[0].first.attack_mask = 0;
        let error = parse(VALID, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("no usable authored move"));
        opponents[0].first.level = 0;
        let error = parse(VALID, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("opponent level 0 outside 1..31"));

        let (creatures, _) = models();
        let member = |id| OpponentPartyMember {
            creature_id: id,
            level: 31,
            attack_mask: 0b00010,
        };
        let other = OpponentSeed {
            first: member(6),
            second: member(6),
            third: member(6),
        };
        let target = OpponentSeed {
            first: member(0),
            second: member(3),
            third: member(6),
        };
        let mut many = vec![other; 255];
        many.push(target);
        let error = parse(VALID, &creatures, &many).unwrap_err();
        assert!(error.0.contains("trainer index 255 is reserved"));
    }

    #[test]
    fn enforces_list_count_limits_and_accepts_count_255() {
        let (creatures, opponents) = models();
        let player_entry = VALID
            .split("[[player_team]]")
            .nth(1)
            .unwrap()
            .split("[[opponent_team]]")
            .next()
            .unwrap();
        let roster = (0..255)
            .map(|i| {
                format!(
                    "[[player_team]]\n{}",
                    player_entry.replace("key = \"starter\"", &format!("key = \"team_{i}\""))
                )
            })
            .collect::<String>();
        let source = format!(
            "label_font = \"../images/ArduFont_5x6.png\"\n{roster}[[opponent_team]]\nkey = \"rival\"\nlabel = \"Rival\"\ntrainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]\n"
        );
        assert_eq!(
            parse(&source, &creatures, &opponents)
                .unwrap()
                .players
                .len(),
            255
        );
        let duplicate_opponents = (0..255).map(|i| format!("[[opponent_team]]\nkey = \"rival_{i}\"\nlabel = \"Rival\"\ntrainer_species = [\"SquibbleSnail\", \"SkitterCrab\", \"squid\"]\n")).collect::<String>();
        let malformed = format!("{source}{duplicate_opponents}");
        let error = parse(&malformed, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("1..=255"));
    }

    #[test]
    fn load_resolves_font_relative_to_source_and_adds_path_to_errors() {
        let (creatures, opponents) = models();
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/arena_demo/arena-demo.toml"
        );
        let catalog = load(path, &creatures, &opponents).unwrap();
        assert!(
            catalog
                .label_font
                .ends_with("testdata/arena_demo/../images/ArduFont_5x6.png")
        );
        let error = load("missing-arena-demo.toml", &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("missing-arena-demo.toml"));
    }
}
