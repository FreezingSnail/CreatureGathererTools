//! Named, test-only battle matchups resolved against canonical project data.
use crate::builder::BuildError;
use crate::model::{attack_mask, Creature, OpponentSeed};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    preset: Vec<SourcePreset>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourcePreset {
    name: String,
    trainer_species: [String; 3],
    player: Vec<SourceMember>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceMember {
    species: String,
    level: u8,
    moves: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    pub species: u8,
    pub level: u8,
    pub attack_mask: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub name: String,
    pub player: [Member; 3],
    pub trainer_id: u8,
}

fn creature_id(name: &str, creatures: &[Creature]) -> Result<u8, BuildError> {
    let matches: Vec<_> = creatures
        .iter()
        .filter(|creature| creature.name.eq_ignore_ascii_case(name))
        .collect();
    if matches.len() != 1 {
        return Err(BuildError(format!(
            "preset species {name:?} resolves to {} canonical creatures",
            matches.len()
        )));
    }
    u8::try_from(matches[0].id)
        .map_err(|_| BuildError(format!("preset species {name:?} has invalid id")))
}

fn valid_symbol(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

pub fn parse(
    source: &str,
    creatures: &[Creature],
    opponents: &[OpponentSeed],
) -> Result<Vec<Preset>, BuildError> {
    let parsed: Source = toml::from_str(source)
        .map_err(|error| BuildError(format!("invalid battle presets TOML: {error}")))?;
    if parsed.preset.is_empty() {
        return Err(BuildError(
            "battle presets must contain at least one [[preset]]".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(parsed.preset.len());
    for entry in parsed.preset {
        if !valid_symbol(&entry.name) || !seen.insert(entry.name.clone()) {
            return Err(BuildError(format!(
                "invalid or duplicate battle preset name {:?}",
                entry.name
            )));
        }
        if entry.player.len() != 3 {
            return Err(BuildError(format!(
                "preset {:?} needs exactly three player members, found {}",
                entry.name,
                entry.player.len()
            )));
        }
        let mut player = Vec::with_capacity(3);
        for member in entry.player {
            if !(1..=31).contains(&member.level) {
                return Err(BuildError(format!(
                    "preset {:?} player level {} outside 1..31",
                    entry.name, member.level
                )));
            }
            player.push(Member {
                species: creature_id(&member.species, creatures)?,
                level: member.level,
                attack_mask: attack_mask(&member.moves),
            });
        }
        let player: [Member; 3] = player.try_into().expect("length checked");
        let trainer_species = entry
            .trainer_species
            .map(|name| creature_id(&name, creatures))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let matching: Vec<_> = opponents
            .iter()
            .enumerate()
            .filter(|(_, seed)| {
                [
                    seed.first.creature_id,
                    seed.second.creature_id,
                    seed.third.creature_id,
                ] == trainer_species.as_slice()
            })
            .collect();
        if matching.len() != 1 {
            return Err(BuildError(format!(
                "preset {:?} trainer species triple resolves to {} opponent rows",
                entry.name,
                matching.len()
            )));
        }
        let trainer_id = u8::try_from(matching[0].0).map_err(|_| {
            BuildError(format!(
                "preset {:?} trainer row exceeds uint8_t",
                entry.name
            ))
        })?;
        result.push(Preset {
            name: entry.name,
            player,
            trainer_id,
        });
    }
    Ok(result)
}

pub fn load(
    path: impl AsRef<Path>,
    creatures: &[Creature],
    opponents: &[OpponentSeed],
) -> Result<Vec<Preset>, BuildError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|error| BuildError(format!("reading {}: {error}", path.display())))?;
    parse(&source, creatures, opponents)
        .map_err(|error| BuildError(format!("{}: {error}", path.display())))
}

pub fn emit_header(presets: &[Preset], path: impl AsRef<Path>) -> Result<(), BuildError> {
    let mut header = String::from(
        "// Generated by cgfx-tools - do not edit.\n\
         #pragma once\n\
         #include <stdint.h>\n\
         #ifdef __AVR__\n\
         #include <avr/pgmspace.h>\n\
         #include <string.h>\n\
         #define BATTLE_PRESET_STORAGE PROGMEM\n\
         #else\n\
         #define BATTLE_PRESET_STORAGE\n\
         #endif\n\
         namespace BattlePresets {\n\
         struct Member { uint8_t species, level, attackMask; };\n\
         struct Preset { Member player[3]; uint8_t trainerId; };\n\
         static_assert(sizeof(Preset) == 10, \"preset layout changed\");\n",
    );
    for preset in presets {
        header.push_str(&format!(
            "const Preset {} BATTLE_PRESET_STORAGE = {{\n    {{\n",
            preset.name
        ));
        for member in preset.player {
            header.push_str(&format!(
                "        {{ {}, {}, {} }},\n",
                member.species, member.level, member.attack_mask
            ));
        }
        header.push_str(&format!("    }},\n    {}\n}};\n", preset.trainer_id));
    }
    header.push_str(
        "inline Preset copyPreset(const Preset &source) {\n\
             Preset value;\n\
             #ifdef __AVR__\n\
             memcpy_P(&value, &source, sizeof(value));\n\
             #else\n\
             value = source;\n\
             #endif\n\
             return value;\n\
         }\n\
         } // namespace BattlePresets\n\
         #undef BATTLE_PRESET_STORAGE\n",
    );
    std::fs::write(path.as_ref(), header)
        .map_err(|error| BuildError(format!("writing {}: {error}", path.as_ref().display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OpponentPartyMember;

    const VALID: &str = r#"
[[preset]]
name = "opening"
trainer_species = ["SquibbleSnail", "SkitterCrab", "squid"]
[[preset.player]]
species = "SquibbleSnail"
level = 31
moves = ["spirit", "water"]
[[preset.player]]
species = "SkitterCrab"
level = 20
moves = ["earth"]
[[preset.player]]
species = "squid"
level = 10
moves = ["none"]
"#;

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
        let member = |creature_id| OpponentPartyMember {
            creature_id,
            level: 31,
            attack_mask: 0b00001,
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
    fn resolves_named_team_and_empty_player_masks() {
        let (creatures, opponents) = models();
        let presets = parse(VALID, &creatures, &opponents).unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].trainer_id, 0);
        assert_eq!(presets[0].player[0].species, 0);
        assert_eq!(presets[0].player[0].attack_mask, 0b00011);
        assert_eq!(presets[0].player[2].attack_mask, 0);
    }

    #[test]
    fn rejects_malformed_or_unresolved_preset_members() {
        reject(&VALID.replace("SkitterCrab", "Unknown"), "preset species");
        reject(&VALID.replace("level = 20", "level = 0"), "outside 1..31");
        reject(&VALID.replace("level = 20", "level = 32"), "outside 1..31");
        reject(
            &VALID.replace("name = \"opening\"", "name = \"\""),
            "invalid or duplicate",
        );
        reject(&format!("{VALID}{VALID}"), "invalid or duplicate");
    }

    #[test]
    fn rejects_missing_and_ambiguous_trainer_triples() {
        reject(
            &VALID.replace("\"squid\"]", "\"SquibbleSnail\"]"),
            "0 opponent rows",
        );
        let (creatures, mut opponents) = models();
        opponents.push(opponents[0]);
        let error = parse(VALID, &creatures, &opponents).unwrap_err();
        assert!(error.0.contains("2 opponent rows"), "{error}");
    }
}
