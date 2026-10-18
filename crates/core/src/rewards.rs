//! First-area material, plant and heal reward mappings
//! (CreatureGathererFX-jp8.11.3).
//!
//! `data/beta-area.toml` is the single authored source for the first-beta
//! content contract. This module derives three item-shaped reward mappings from
//! it and validates them before the generator packs any byte:
//!
//! * ordinary-wild materials: every `encounters.allowed_species` resolves to one
//!   tier-2 lure. The lure type is the authored primary type folded into the
//!   four lure-zone groups (`type & 3`), which is exactly how the deposit path
//!   reduces an item lure id into a group (`LureZone::lureType`). Item id is
//!   `lure_type * 3 + 2`, the shared `item::lureId` layout.
//! * pocket plant: `plant_source` names the tier-1 lure item and its count.
//! * selected heals: `[[reward.heal]]` names consumable drops, checked against
//!   the canonical `data/consumables.csv` (Heal kind only).
//!
//! The mapping resolves on a single interaction/battle transition read; it is
//! packed append-only so no earlier FX field address moves.

use crate::builder::{BuildError, ConsumableKind, ConsumableTable};
use crate::model::{fold_type_name, Creature};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

/// Lure id space: `LURE_TYPE_COUNT * LURE_TIER_COUNT` (8 types x 3 tiers).
pub const LURE_COUNT: u8 = 24;
pub const LURE_TIER_COUNT: u8 = 3;
/// Tier of every ordinary-wild material.
pub const MATERIAL_TIER: u8 = 2;
/// Item count cap shared with the runtime inventory.
pub const COUNT_MAX: u8 = 99;
/// Lure types 0..=3 map onto the four supported lure-zone encounter groups.
pub const SUPPORTED_LURE_TYPES: u8 = 4;
/// Sentinel stored for a species with no authored material.
pub const NO_MATERIAL: u8 = 0xFF;
pub const CONSUMABLE_COUNT: u8 = 8;

const CREATURE_TYPE_INDEX: [(&str, u8); 8] = [
    ("spirit", 0),
    ("water", 1),
    ("wind", 2),
    ("earth", 3),
    ("fire", 4),
    ("lightning", 5),
    ("plant", 6),
    ("elder", 7),
];

fn creature_type_index(name: &str) -> Option<u8> {
    CREATURE_TYPE_INDEX
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, index)| *index)
}

#[derive(Debug, Deserialize)]
struct Source {
    encounters: SourceEncounters,
    plant_source: SourcePlant,
    #[serde(default)]
    reward: Option<SourceReward>,
}

#[derive(Debug, Deserialize)]
struct SourceEncounters {
    allowed_species: Vec<i32>,
}

#[derive(Debug, Deserialize)]
struct SourcePlant {
    lure_type: u8,
    lure_tier: u8,
    item_id: u8,
    count: u8,
}

#[derive(Debug, Deserialize)]
struct SourceReward {
    #[serde(default)]
    heal: Vec<SourceHeal>,
}

#[derive(Debug, Deserialize)]
struct SourceHeal {
    item_id: u8,
    count: u8,
}

/// One ordinary-wild species material: a tier-2 lure and its count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaterialReward {
    pub species: u8,
    pub item: u8,
    pub count: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HealReward {
    pub item: u8,
    pub count: u8,
}

/// Validated first-area reward mapping. `material_item`/`material_count` are
/// indexed by species id 0..`species_count`, with `NO_MATERIAL`/0 for species
/// outside the authored first area.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardMapping {
    pub materials: Vec<MaterialReward>,
    pub material_item: Vec<u8>,
    pub material_count: Vec<u8>,
    pub plant_item: u8,
    pub plant_count: u8,
    pub heals: Vec<HealReward>,
    pub species_count: u8,
}

fn lure_item(lure_type: u8, tier: u8) -> u8 {
    lure_type * LURE_TIER_COUNT + tier
}

/// Parses the authored contract and validates the three reward mappings.
/// `creatures` is the canonical species order; `consumables` is the canonical
/// consumable table used to resolve selected heal drops.
pub fn parse(
    source: &str,
    creatures: &[Creature],
    consumables: &ConsumableTable,
) -> Result<RewardMapping, BuildError> {
    let parsed: Source = toml::from_str(source)
        .map_err(|error| BuildError(format!("invalid beta-area contract TOML: {error}")))?;

    let species_count = u8::try_from(creatures.len())
        .map_err(|_| BuildError("creature count does not fit the species byte".into()))?;
    if species_count == 0 || species_count == NO_MATERIAL {
        return Err(BuildError(format!(
            "species count {species_count} is outside 1..{NO_MATERIAL}"
        )));
    }
    let by_id: std::collections::HashMap<i32, &Creature> =
        creatures.iter().map(|c| (c.id, c)).collect();

    // --- ordinary-wild tier-2 materials -----------------------------------
    let allowed = &parsed.encounters.allowed_species;
    if allowed.is_empty() {
        return Err(BuildError(
            "contract encounters.allowed_species must name at least one species".into(),
        ));
    }
    let mut seen = HashSet::new();
    let mut materials = Vec::with_capacity(allowed.len());
    for &species in allowed {
        if !seen.insert(species) {
            return Err(BuildError(format!(
                "duplicate first-area source identity: species {species} is authored twice"
            )));
        }
        let creature = by_id.get(&species).ok_or_else(|| {
            BuildError(format!(
                "first-area material species {species} does not exist in the canonical roster"
            ))
        })?;
        let species_id = u8::try_from(species)
            .map_err(|_| BuildError(format!("species {species} id does not fit a byte")))?;
        let type_index = creature_type_index(&creature.type1).ok_or_else(|| {
            BuildError(format!(
                "species {species} has unknown primary type {:?}",
                creature.type1
            ))
        })?;
        if creature.moves.is_empty() {
            return Err(BuildError(format!(
                "first-area material species {species} has no usable attack type"
            )));
        }
        for move_type in &creature.moves {
            if fold_type_name(move_type).is_none() {
                return Err(BuildError(format!(
                    "first-area material species {species} references unknown attack type {move_type:?}"
                )));
            }
        }
        let lure_type = type_index & (SUPPORTED_LURE_TYPES - 1);
        if lure_type >= SUPPORTED_LURE_TYPES {
            return Err(BuildError(format!(
                "species {species} derived lure type {lure_type} is outside 0..{SUPPORTED_LURE_TYPES}"
            )));
        }
        let item = lure_item(lure_type, MATERIAL_TIER);
        if item >= LURE_COUNT {
            return Err(BuildError(format!(
                "species {species} material item {item} is outside the lure id space"
            )));
        }
        materials.push(MaterialReward {
            species: species_id,
            item,
            count: 1,
        });
    }

    let mut material_item = vec![NO_MATERIAL; species_count as usize];
    let mut material_count = vec![0u8; species_count as usize];
    for material in &materials {
        material_item[material.species as usize] = material.item;
        material_count[material.species as usize] = material.count;
    }

    // --- pocket plant tier-1 source ---------------------------------------
    let plant = &parsed.plant_source;
    if plant.lure_type >= SUPPORTED_LURE_TYPES {
        return Err(BuildError(format!(
            "plant source lure type {} is outside 0..{SUPPORTED_LURE_TYPES}",
            plant.lure_type
        )));
    }
    if plant.lure_tier >= LURE_TIER_COUNT {
        return Err(BuildError(format!(
            "plant source lure tier {} is outside 0..{LURE_TIER_COUNT}",
            plant.lure_tier
        )));
    }
    let expected_plant_item = lure_item(plant.lure_type, plant.lure_tier);
    if plant.item_id != expected_plant_item || plant.item_id >= LURE_COUNT {
        return Err(BuildError(format!(
            "plant source item id {} does not match lure type {} tier {} (expected {expected_plant_item})",
            plant.item_id, plant.lure_type, plant.lure_tier
        )));
    }
    if plant.count < 1 || plant.count > COUNT_MAX {
        return Err(BuildError(format!(
            "plant source count {} is outside 1..{COUNT_MAX}",
            plant.count
        )));
    }

    // --- selected heal rewards --------------------------------------------
    let heals_src = parsed
        .reward
        .as_ref()
        .map(|reward| reward.heal.as_slice())
        .unwrap_or_default();
    let mut heal_seen = HashSet::new();
    let mut heals = Vec::with_capacity(heals_src.len());
    for heal in heals_src {
        if heal.item_id >= CONSUMABLE_COUNT {
            return Err(BuildError(format!(
                "heal reward consumable id {} is outside 0..{CONSUMABLE_COUNT}",
                heal.item_id
            )));
        }
        let def = consumables.rows[heal.item_id as usize];
        if def.kind != ConsumableKind::Heal {
            return Err(BuildError(format!(
                "heal reward consumable {} is not a Heal definition",
                heal.item_id
            )));
        }
        if !heal_seen.insert(heal.item_id) {
            return Err(BuildError(format!(
                "duplicate heal reward source identity: consumable {} is authored twice",
                heal.item_id
            )));
        }
        if heal.count < 1 || heal.count > COUNT_MAX {
            return Err(BuildError(format!(
                "heal reward consumable {} count {} is outside 1..{COUNT_MAX}",
                heal.item_id, heal.count
            )));
        }
        heals.push(HealReward {
            item: heal.item_id,
            count: heal.count,
        });
    }

    Ok(RewardMapping {
        materials,
        material_item,
        material_count,
        plant_item: plant.item_id,
        plant_count: plant.count,
        heals,
        species_count,
    })
}

/// Loads and validates the contract at `path`.
pub fn load(
    path: impl AsRef<Path>,
    creatures: &[Creature],
    consumables: &ConsumableTable,
) -> Result<RewardMapping, BuildError> {
    let path = path.as_ref();
    let source = std::fs::read_to_string(path)
        .map_err(|error| BuildError(format!("reading {}: {error}", path.display())))?;
    parse(&source, creatures, consumables)
        .map_err(|error| BuildError(format!("{}: {error}", path.display())))
}

fn byte_array(name: &str, values: &[u8]) -> String {
    let joined = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("uint8_t {name}[] = {{ {joined} }};\n")
}

/// Builds `rewards.txt`, the append-only `Rewards` FX descriptor consumed by
/// `fxlayout.toml`.
pub fn build_fx_reward_descriptor(mapping: &RewardMapping) -> String {
    let mut output = String::from(
        "// Generated by cgfx-tools from data/beta-area.toml.\n\
         // Do not edit: regenerate with make gen.\n\
         namespace Rewards {\n",
    );
    output.push_str(&byte_array("materialItem", &mapping.material_item));
    output.push_str(&byte_array("materialCount", &mapping.material_count));
    output.push_str(&byte_array("plantItem", &[mapping.plant_item]));
    output.push_str(&byte_array("plantCount", &[mapping.plant_count]));
    let heal_items: Vec<u8> = mapping.heals.iter().map(|heal| heal.item).collect();
    let heal_counts: Vec<u8> = mapping.heals.iter().map(|heal| heal.count).collect();
    output.push_str(&byte_array("healItem", &heal_items));
    output.push_str(&byte_array("healCount", &heal_counts));
    output.push_str("} namespace_end\n");
    output
}

/// Writes `rewards.txt` into `output_dir`.
pub fn write_reward_descriptor(
    output_dir: impl AsRef<Path>,
    mapping: &RewardMapping,
) -> Result<(), BuildError> {
    let output_dir = output_dir.as_ref();
    std::fs::create_dir_all(output_dir)
        .map_err(|error| BuildError(format!("creating {}: {error}", output_dir.display())))?;
    let path = output_dir.join("rewards.txt");
    std::fs::write(&path, build_fx_reward_descriptor(mapping))
        .map_err(|error| BuildError(format!("writing {}: {error}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::ConsumableDef;

    const CONTRACT: &str = r#"
contract = "first-beta"
[area]
min_x = 8
[encounters]
allowed_species = [0, 3, 6, 9, 10, 12, 13, 14]
[[encounters.table]]
name = "base"
species = [0, 3, 6, 9, 10, 12, 13, 14, 0, 6]
[plant_source]
lure_type = 0
lure_tier = 1
item_id = 1
count = 1
[[reward.heal]]
item_id = 0
count = 1
[[reward.heal]]
item_id = 1
count = 1
"#;

    fn creature(id: i32, type1: &str, moves: &[&str]) -> Creature {
        Creature {
            name: format!("species{id}"),
            id,
            type1: type1.into(),
            type2: "none".into(),
            evo_level: 8,
            hp: 1,
            evolves_to: None,
            moves: moves.iter().map(|m| (*m).to_string()).collect(),
        }
    }

    fn roster() -> Vec<Creature> {
        let mut roster = Vec::new();
        for id in 0..16 {
            roster.push(creature(id, "wind", &["water"]));
        }
        // Overwrite the authored first-area species' primary types.
        let set = |roster: &mut Vec<Creature>, id: i32, type1: &str| {
            roster[id as usize].type1 = type1.into();
        };
        set(&mut roster, 0, "wind");
        set(&mut roster, 3, "earth");
        set(&mut roster, 6, "water");
        set(&mut roster, 9, "spirit");
        set(&mut roster, 10, "fire");
        set(&mut roster, 12, "plant");
        set(&mut roster, 13, "wind");
        set(&mut roster, 14, "earth");
        roster
    }

    fn consumables() -> ConsumableTable {
        let mut rows = [ConsumableDef {
            kind: ConsumableKind::None,
            arg: 0,
        }; 8];
        rows[0] = ConsumableDef {
            kind: ConsumableKind::Heal,
            arg: 16,
        };
        rows[1] = ConsumableDef {
            kind: ConsumableKind::Heal,
            arg: 32,
        };
        ConsumableTable { rows }
    }

    fn mapping() -> RewardMapping {
        parse(CONTRACT, &roster(), &consumables()).expect("valid contract")
    }

    #[test]
    fn derives_tier2_materials_for_every_allowed_species() {
        let mapping = mapping();
        assert_eq!(mapping.materials.len(), 8);
        // lure_type = primary type index & 3; item = lure_type * 3 + 2.
        let expected = [
            (0u8, 2u8, 8u8),
            (3, 3, 11),
            (6, 1, 5),
            (9, 0, 2),
            (10, 0, 2),
            (12, 2, 8),
            (13, 2, 8),
            (14, 3, 11),
        ];
        for (species, lure_type, item) in expected {
            let material = mapping
                .materials
                .iter()
                .find(|m| m.species == species)
                .expect("species material");
            assert_eq!(material.item, item, "species {species} item");
            assert_eq!(material.count, 1, "species {species} count");
            assert_eq!(material.item % LURE_TIER_COUNT, MATERIAL_TIER);
            assert_eq!(material.item / LURE_TIER_COUNT, lure_type);
        }
        assert_eq!(mapping.material_item[0], 8);
        assert_eq!(mapping.material_item[1], NO_MATERIAL, "unauthored species sentinel");
        assert_eq!(mapping.material_count[1], 0);
    }

    #[test]
    fn plant_and_heal_mappings_match_the_contract() {
        let mapping = mapping();
        assert_eq!(mapping.plant_item, 1);
        assert_eq!(mapping.plant_count, 1);
        assert_eq!(mapping.plant_item / LURE_TIER_COUNT, 0);
        assert_eq!(mapping.heals, vec![
            HealReward { item: 0, count: 1 },
            HealReward { item: 1, count: 1 }
        ]);
    }

    #[test]
    fn descriptor_pins_every_array() {
        let descriptor = build_fx_reward_descriptor(&mapping());
        assert!(descriptor.contains("namespace Rewards {"), "{descriptor}");
        assert!(
            descriptor.contains("uint8_t plantItem[] = { 1 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t plantCount[] = { 1 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t healItem[] = { 0, 1 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t materialItem[] = { 8, 255, 255, 11,"),
            "{descriptor}"
        );
        assert!(descriptor.ends_with("} namespace_end\n"), "{descriptor}");
    }

    #[test]
    fn duplicate_source_identity_is_rejected() {
        let contract = CONTRACT.replace(
            "allowed_species = [0, 3, 6, 9, 10, 12, 13, 14]",
            "allowed_species = [0, 0, 3, 6, 9, 10, 12, 13, 14]",
        );
        let error = parse(&contract, &roster(), &consumables()).unwrap_err();
        assert!(error.0.contains("duplicate first-area source identity"), "{error}");
    }

    #[test]
    fn missing_species_and_unknown_type_are_rejected() {
        let contract = CONTRACT.replace("allowed_species = [0, 3", "allowed_species = [99, 0, 3");
        let error = parse(&contract, &roster(), &consumables()).unwrap_err();
        assert!(error.0.contains("does not exist"), "{error}");

        let mut roster = roster();
        roster[3].type1 = "mystery".into();
        let error = parse(CONTRACT, &roster, &consumables()).unwrap_err();
        assert!(error.0.contains("unknown primary type"), "{error}");
    }

    #[test]
    fn plant_and_heal_bounds_are_rejected() {
        let bad_item = CONTRACT.replace(
            "[plant_source]\nlure_type = 0\nlure_tier = 1\nitem_id = 1",
            "[plant_source]\nlure_type = 0\nlure_tier = 1\nitem_id = 2",
        );
        let error = parse(&bad_item, &roster(), &consumables()).unwrap_err();
        assert!(error.0.contains("does not match lure type"), "{error}");

        let bad_count = CONTRACT.replace(
            "[plant_source]\nlure_type = 0\nlure_tier = 1\nitem_id = 1\ncount = 1",
            "[plant_source]\nlure_type = 0\nlure_tier = 1\nitem_id = 1\ncount = 100",
        );
        let error = parse(&bad_count, &roster(), &consumables()).unwrap_err();
        assert!(error.0.contains("count 100 is outside"), "{error}");

        // Consumable 3 is a Cure, so it cannot be a selected heal reward.
        let bad_heal = CONTRACT.replace(
            "[[reward.heal]]\nitem_id = 1\ncount = 1",
            "[[reward.heal]]\nitem_id = 3\ncount = 1",
        );
        let error = parse(&bad_heal, &roster(), &consumables()).unwrap_err();
        assert!(error.0.contains("not a Heal definition"), "{error}");

        let dup_heal = CONTRACT.replace(
            "[[reward.heal]]\nitem_id = 1\ncount = 1",
            "[[reward.heal]]\nitem_id = 0\ncount = 1",
        );
        let error = parse(&dup_heal, &roster(), &consumables()).unwrap_err();
        assert!(error.0.contains("duplicate heal reward source identity"), "{error}");
    }
}
