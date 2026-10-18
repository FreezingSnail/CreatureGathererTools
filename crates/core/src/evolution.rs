//! Explicit first-stage evolution edges (CreatureGathererFX-jp8.11.11).
//!
//! The canonical creature authoring data (`data/json/creatures.json`) carries
//! `evelevel` (the level threshold) and an explicit `evolves_to` target. The
//! relation is not inferred from ids: `id + 1` is not a general species
//! relation, so each evolving species names its target. The native authoring
//! tool derives the edge list from those targets and validates it before it
//! emits the packed mapping or the device fixture.

use crate::builder::BuildError;
use crate::model::{Creature, fold_type_name};
use std::collections::{HashMap, HashSet};

/// Highest supported device level; the authoring threshold must fit it.
pub const MAX_LEVEL: i32 = 31;

/// One authored `source -> target` evolution edge at `level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvolutionEdge {
    pub from_id: i32,
    pub to_id: i32,
    pub level: i32,
}

/// Derives the authored edge list in creature declaration order. Species with
/// no `evolves_to` target contribute no edge.
pub fn evolution_edges(creatures: &[Creature]) -> Vec<EvolutionEdge> {
    creatures
        .iter()
        .filter_map(|creature| {
            creature.evolves_to.map(|to_id| EvolutionEdge {
                from_id: creature.id,
                to_id,
                level: creature.evo_level,
            })
        })
        .collect()
}

/// Validates every authored edge and returns it in declaration order.
///
/// Rejects: a duplicate source (two entries for one species), a missing or
/// self-referential target, a threshold outside `1..=MAX_LEVEL`, a target with
/// no usable attack type, and any cycle over the edges. Called before the
/// generator writes or packs the mapping, so a bad authoring file fails
/// generation rather than shipping.
pub fn validate_evolutions(creatures: &[Creature]) -> Result<Vec<EvolutionEdge>, BuildError> {
    let edges = evolution_edges(creatures);
    let by_id: HashMap<i32, &Creature> = creatures.iter().map(|c| (c.id, c)).collect();

    let mut seen_sources: HashSet<i32> = HashSet::new();
    for edge in &edges {
        if !seen_sources.insert(edge.from_id) {
            return Err(BuildError(format!(
                "duplicate evolution edge: species {} is authored twice",
                edge.from_id
            )));
        }
        let source = by_id.get(&edge.from_id).expect("edge source is a creature");
        if edge.level < 1 || edge.level > MAX_LEVEL {
            return Err(BuildError(format!(
                "species {} evolution threshold {} is outside 1..={MAX_LEVEL}",
                edge.from_id, edge.level
            )));
        }
        if edge.level != source.evo_level {
            return Err(BuildError(format!(
                "species {} evolution threshold {} does not match authored evelevel {}",
                edge.from_id, edge.level, source.evo_level
            )));
        }
        if edge.from_id == edge.to_id {
            return Err(BuildError(format!(
                "species {} evolution target must differ from its source",
                edge.from_id
            )));
        }
        let target = by_id.get(&edge.to_id).ok_or_else(|| {
            BuildError(format!(
                "species {} evolution target {} does not exist",
                edge.from_id, edge.to_id
            ))
        })?;
        if target.moves.is_empty() {
            return Err(BuildError(format!(
                "species {} evolution target {} has no usable attack type",
                edge.from_id, edge.to_id
            )));
        }
        for move_type in &target.moves {
            if fold_type_name(move_type).is_none() {
                return Err(BuildError(format!(
                    "species {} evolution target {} references unknown attack type {move_type:?}",
                    edge.from_id, edge.to_id
                )));
            }
        }
    }

    for edge in &edges {
        let mut current = edge.to_id;
        while let Some(creature) = by_id.get(&current) {
            match creature.evolves_to {
                Some(next) if next == edge.from_id => {
                    return Err(BuildError(format!(
                        "cyclic evolution edges involve species {}",
                        edge.from_id
                    )));
                }
                Some(next) => current = next,
                None => break,
            }
        }
    }

    Ok(edges)
}

fn evolution_array(name: &str, values: impl Iterator<Item = i32>) -> String {
    let joined = values
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!("uint8_t {name}[] = {{ {joined} }};\n")
}

/// Builds `evolution.txt`, the append-only `Evolution` FX descriptor consumed
/// by `fxlayout.toml`. The declarations mirror the validated authored edges in
/// declaration order.
pub fn build_fx_evolution_descriptor(creatures: &[Creature]) -> Result<String, BuildError> {
    let edges = validate_evolutions(creatures)?;
    let mut output = String::from(
        "// Generated by cgfx-tools from data/json/creatures.json.\n\
         // Do not edit: regenerate with make gen.\n\
         namespace Evolution {\n",
    );
    output.push_str(&evolution_array(
        "evolutionSource",
        edges.iter().map(|edge| edge.from_id),
    ));
    output.push_str(&evolution_array(
        "evolutionTarget",
        edges.iter().map(|edge| edge.to_id),
    ));
    output.push_str(&evolution_array(
        "evolutionLevel",
        edges.iter().map(|edge| edge.level),
    ));
    output.push_str("} namespace_end\n");
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Creature;

    fn creature(id: i32, evo_level: i32, evolves_to: Option<i32>, moves: &[&str]) -> Creature {
        Creature {
            name: format!("species{id}"),
            id,
            type1: "wind".into(),
            type2: "none".into(),
            evo_level,
            hp: 1,
            evolves_to,
            moves: moves.iter().map(|m| (*m).to_string()).collect(),
        }
    }

    fn first_beta_edges() -> Vec<Creature> {
        vec![
            creature(0, 8, Some(1), &["water"]),
            creature(1, 16, None, &["water"]),
            creature(3, 8, Some(4), &["earth"]),
            creature(4, 16, None, &["earth", "fire"]),
            creature(6, 8, Some(7), &["water"]),
            creature(7, 16, None, &["water"]),
        ]
    }

    fn chained_edges() -> Vec<Creature> {
        // Full three-link starter chains 0->1->2, 3->4->5, 6->7->8.
        vec![
            creature(0, 8, Some(1), &["water"]),
            creature(1, 16, Some(2), &["water"]),
            creature(2, 24, None, &["water", "plant"]),
            creature(3, 8, Some(4), &["earth"]),
            creature(4, 16, Some(5), &["earth", "fire"]),
            creature(5, 24, None, &["earth", "lightning"]),
            creature(6, 8, Some(7), &["water"]),
            creature(7, 16, Some(8), &["water"]),
            creature(8, 24, None, &["water", "fire"]),
        ]
    }

    #[test]
    fn derives_exactly_the_three_first_beta_edges_in_order() {
        let edges = validate_evolutions(&first_beta_edges()).expect("valid edges");
        assert_eq!(
            edges,
            vec![
                EvolutionEdge { from_id: 0, to_id: 1, level: 8 },
                EvolutionEdge { from_id: 3, to_id: 4, level: 8 },
                EvolutionEdge { from_id: 6, to_id: 7, level: 8 },
            ]
        );
    }

    #[test]
    fn descriptor_pins_source_target_and_level_arrays() {
        let descriptor = build_fx_evolution_descriptor(&first_beta_edges()).expect("descriptor");
        assert!(descriptor.contains("namespace Evolution {"), "{descriptor}");
        assert!(
            descriptor.contains("uint8_t evolutionSource[] = { 0, 3, 6 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t evolutionTarget[] = { 1, 4, 7 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t evolutionLevel[] = { 8, 8, 8 };"),
            "{descriptor}"
        );
        assert!(descriptor.ends_with("} namespace_end\n"), "{descriptor}");
    }

    #[test]
    fn accepts_three_link_chains_and_pins_their_arrays() {
        let edges = validate_evolutions(&chained_edges()).expect("valid chains");
        assert_eq!(
            edges,
            vec![
                EvolutionEdge { from_id: 0, to_id: 1, level: 8 },
                EvolutionEdge { from_id: 1, to_id: 2, level: 16 },
                EvolutionEdge { from_id: 3, to_id: 4, level: 8 },
                EvolutionEdge { from_id: 4, to_id: 5, level: 16 },
                EvolutionEdge { from_id: 6, to_id: 7, level: 8 },
                EvolutionEdge { from_id: 7, to_id: 8, level: 16 },
            ]
        );
        let descriptor = build_fx_evolution_descriptor(&chained_edges()).expect("descriptor");
        assert!(
            descriptor.contains("uint8_t evolutionSource[] = { 0, 1, 3, 4, 6, 7 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t evolutionTarget[] = { 1, 2, 4, 5, 7, 8 };"),
            "{descriptor}"
        );
        assert!(
            descriptor.contains("uint8_t evolutionLevel[] = { 8, 16, 8, 16, 8, 16 };"),
            "{descriptor}"
        );
    }

    #[test]
    fn a_chain_middle_is_not_rejected_as_a_cycle() {
        // 0->1->2 walks forward and terminates: a chain, never a cycle.
        assert!(validate_evolutions(&chained_edges()).is_ok());
    }

    #[test]
    fn a_back_edge_in_a_chain_is_rejected() {
        let mut creatures = chained_edges();
        // species 7 already evolves to 8; point it back at 6 to close 6->7->6.
        creatures[7].evolves_to = Some(6);
        let error = validate_evolutions(&creatures).expect_err("back edge must fail");
        assert!(error.0.contains("cyclic evolution edges"), "{error}");
    }

    #[test]
    fn missing_target_is_rejected() {
        let mut creatures = first_beta_edges();
        creatures[0].evolves_to = Some(99);
        let error = validate_evolutions(&creatures).expect_err("missing target must fail");
        assert!(error.0.contains("target 99 does not exist"), "{error}");
    }

    #[test]
    fn self_referential_target_is_rejected() {
        let mut creatures = first_beta_edges();
        creatures[0].evolves_to = Some(0);
        let error = validate_evolutions(&creatures).expect_err("self loop must fail");
        assert!(error.0.contains("must differ from its source"), "{error}");
    }

    #[test]
    fn duplicate_source_is_rejected() {
        let mut creatures = first_beta_edges();
        // Two records claim species 0; the second repeats the target.
        creatures.push(creature(0, 8, Some(1), &["water"]));
        let error = validate_evolutions(&creatures).expect_err("duplicate source must fail");
        assert!(error.0.contains("duplicate evolution edge"), "{error}");
    }

    #[test]
    fn cyclic_edges_are_rejected() {
        let mut creatures = first_beta_edges();
        creatures[1].evolves_to = Some(0);
        let error = validate_evolutions(&creatures).expect_err("cycle must fail");
        assert!(error.0.contains("cyclic evolution edges"), "{error}");
    }

    #[test]
    fn zero_threshold_is_rejected() {
        let mut creatures = first_beta_edges();
        creatures[0].evo_level = 0;
        let error = validate_evolutions(&creatures).expect_err("zero threshold must fail");
        assert!(error.0.contains("outside 1..=31"), "{error}");
    }

    #[test]
    fn unknown_target_attack_type_is_rejected() {
        let mut creatures = first_beta_edges();
        creatures[1].moves = vec!["mystery".to_string()];
        let error = validate_evolutions(&creatures).expect_err("bad move must fail");
        assert!(error.0.contains("unknown attack type"), "{error}");
    }

    #[test]
    fn target_without_usable_attack_is_rejected() {
        let mut creatures = first_beta_edges();
        creatures[1].moves = vec![];
        let error = validate_evolutions(&creatures).expect_err("empty moves must fail");
        assert!(error.0.contains("no usable attack type"), "{error}");
    }
}
