pub mod arena;
pub mod arena_demo;
pub mod battle_presets;
pub mod blocks;
pub mod builder;
pub mod cli;
pub mod config;
pub mod duel;
pub mod evolution;
pub mod fixtures;
pub mod fxpack;
pub mod manifest;
pub mod model;
pub mod parser;
pub mod processor;
pub mod rewards;
pub mod script;
pub mod store;
pub mod tiled;
pub mod wfcmap;
pub mod writer;
use anyhow::Context;
use clap::Parser;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use cgfx_wfc::WfcError;
use serde::Serialize;

use crate::cli::{Command, WfcCommand, WfcGenerate};
use crate::model::GameMap;
use crate::wfcmap::{
    GenerateError, Stamp, StampOrigin, WfcProject, compile_recipe_for_size, encode, generate,
};

/// A command-specific failure with a stable process exit code.
#[derive(Debug)]
pub struct CliFailure {
    code: i32,
    message: String,
}

impl CliFailure {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: 2,
            message: message.into(),
        }
    }

    fn contradiction(message: impl Into<String>) -> Self {
        Self {
            code: 3,
            message: message.into(),
        }
    }

    fn budget(message: impl Into<String>) -> Self {
        Self {
            code: 4,
            message: message.into(),
        }
    }
}

impl fmt::Display for CliFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for CliFailure {}

/// Returns the WFC command's documented status code, or the legacy error code.
pub fn exit_code(error: &anyhow::Error) -> i32 {
    error
        .downcast_ref::<CliFailure>()
        .map_or(1, |error| error.code)
}

#[derive(Serialize)]
struct HunkDump {
    w: u32,
    h: u32,
    slot_w: u32,
    slot_h: u32,
    tiles: String,
    seed: u64,
    attempts: u32,
}

fn run_command(command: Command) -> anyhow::Result<()> {
    match command {
        Command::Wfc(WfcCommand::Generate(args)) => run_wfc_generate(args),
    }
}

fn run_wfc_generate(args: WfcGenerate) -> anyhow::Result<()> {
    if !args.dry_run && args.out.is_none() && !args.in_place {
        return Err(CliFailure::invalid("specify --out <FILE> or --in-place").into());
    }
    if args.in_place && args.stamp_name.is_none() && !args.dry_run {
        return Err(CliFailure::invalid("--in-place requires --stamp-name NAME").into());
    }

    let bytes =
        std::fs::read(&args.map).with_context(|| format!("reading map {}", args.map.display()))?;
    let mut map: GameMap = serde_json::from_slice(&bytes).map_err(|error| {
        CliFailure::invalid(format!("parsing map {}: {error}", args.map.display()))
    })?;
    let project = map.wfc.as_ref().ok_or_else(|| {
        CliFailure::invalid(format!(
            "map {} does not contain a WFC project",
            args.map.display()
        ))
    })?;
    project.validate().map_err(validation_failure)?;
    let recipe_id = resolve_recipe(project, &args.recipe)?;
    let recipe = project
        .recipes
        .iter()
        .find(|recipe| recipe.id == recipe_id)
        .expect("resolved recipe exists");
    let recipe_name = recipe.name.clone();
    let hunk = generate(project, recipe_id, args.size, args.seed).map_err(generate_failure)?;
    let compiled = compile_recipe_for_size(
        project,
        recipe,
        args.size.unwrap_or((recipe.size_w, recipe.size_h)),
    )
    .map_err(generate_failure)?;

    let slot_names: Vec<&str> = hunk
        .slots
        .iter()
        .map(|&symbol| {
            let piece_id = compiled
                .symbols
                .get(symbol as usize)
                .expect("generator only returns compiled symbols")
                .piece_id;
            project
                .pieces
                .iter()
                .find(|piece| piece.id == piece_id)
                .expect("compiled symbols originate from project pieces")
                .name
                .as_str()
        })
        .collect();
    let mut counts = BTreeMap::<&str, u32>::new();
    for &name in &slot_names {
        *counts.entry(name).or_default() += 1;
    }

    if args.dry_run {
        print_dry_run(&slot_names, &counts, hunk.w / hunk.slot_w);
    } else if let Some(path) = args.out {
        let dump = HunkDump {
            w: hunk.w,
            h: hunk.h,
            slot_w: hunk.slot_w,
            slot_h: hunk.slot_h,
            tiles: encode(&hunk.tiles),
            seed: hunk.seed,
            attempts: hunk.attempts,
        };
        let dump = serde_json::to_string_pretty(&dump).context("serializing generated hunk")?;
        std::fs::write(&path, dump).with_context(|| format!("writing hunk {}", path.display()))?;
    } else {
        let stamp_name = args.stamp_name.expect("checked before map mutation");
        let stamp_id = map
            .stamps
            .iter()
            .map(|stamp| stamp.id)
            .max()
            .map_or(Ok(1), |id| {
                id.checked_add(1)
                    .ok_or_else(|| CliFailure::invalid("stamp ID overflow"))
            })?;
        map.stamps.push(Stamp {
            id: stamp_id,
            name: stamp_name,
            w: hunk.w,
            h: hunk.h,
            tiles: encode(&hunk.tiles),
            origin: Some(StampOrigin {
                recipe_id,
                seed: hunk.seed,
                slot_w: hunk.slot_w,
                slot_h: hunk.slot_h,
            }),
        });
        let saved =
            serde_json::to_string_pretty(&map).context("serializing map with generated stamp")?;
        std::fs::write(&args.map, saved)
            .with_context(|| format!("writing map {}", args.map.display()))?;
    }

    println!(
        "wfc generate: recipe {}; seed {}; attempts {}; hunk {}x{} tiles ({}x{} slots); {} distinct pieces",
        recipe_name,
        hunk.seed,
        hunk.attempts,
        hunk.w,
        hunk.h,
        hunk.w / hunk.slot_w,
        hunk.h / hunk.slot_h,
        counts.len(),
    );
    Ok(())
}

fn print_dry_run(slot_names: &[&str], counts: &BTreeMap<&str, u32>, slots_w: u32) {
    println!("slot grid:");
    for row in slot_names.chunks(slots_w as usize) {
        println!("{}", row.join(" "));
    }
    println!("piece counts:");
    for (name, count) in counts {
        println!("{name}: {count}");
    }
}

fn resolve_recipe(project: &WfcProject, requested: &str) -> Result<u32, CliFailure> {
    if let Ok(id) = requested.parse::<u32>()
        && project.recipes.iter().any(|recipe| recipe.id == id)
    {
        return Ok(id);
    }
    let matches: Vec<_> = project
        .recipes
        .iter()
        .filter(|recipe| recipe.name == requested)
        .collect();
    match matches.as_slice() {
        [recipe] => Ok(recipe.id),
        [] => Err(CliFailure::invalid(format!(
            "unknown WFC recipe {requested:?}; available recipes: {}",
            recipe_names(project)
        ))),
        _ => Err(CliFailure::invalid(format!(
            "ambiguous WFC recipe name {requested:?}; candidates: {}",
            matches
                .iter()
                .map(|recipe| format!("{} ({})", recipe.name, recipe.id))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

fn recipe_names(project: &WfcProject) -> String {
    let names: Vec<_> = project
        .recipes
        .iter()
        .map(|recipe| recipe.name.as_str())
        .collect();
    if names.is_empty() {
        "(none)".to_string()
    } else {
        names.join(", ")
    }
}

fn validation_failure(issues: Vec<String>) -> CliFailure {
    CliFailure::invalid(format!("invalid WFC project:\n{}", issues.join("\n")))
}

fn generate_failure(error: GenerateError) -> CliFailure {
    match &error {
        GenerateError::Validation(issues) => validation_failure(issues.clone()),
        GenerateError::UnknownRecipe(_) | GenerateError::NoPieces => {
            CliFailure::invalid(error.to_string())
        }
        GenerateError::AnchorOutOfBounds { .. } | GenerateError::AnchorOverlap { .. } => {
            CliFailure::invalid(error.to_string())
        }
        GenerateError::RuleUnsatisfied { .. } | GenerateError::LimitUnsatisfied { .. } => {
            CliFailure::budget(error.to_string())
        }
        GenerateError::Solver(WfcError::Contradiction { .. }) => {
            CliFailure::contradiction(error.to_string())
        }
        GenerateError::Solver(WfcError::MemoryBudget { .. } | WfcError::BudgetExceeded { .. }) => {
            CliFailure::budget(error.to_string())
        }
        GenerateError::Solver(_) => CliFailure::invalid(error.to_string()),
    }
}

pub fn run() -> anyhow::Result<()> {
    let args = cli::Cli::parse();

    if let Some(command) = args.command {
        return run_command(command);
    }

    if let Some(layouts) = args.assert_layout_equivalent {
        fxpack::equivalence::assert_layout_files_equivalent(&layouts[0], &layouts[1])?;
        println!("layout equivalence: PASS");
        return Ok(());
    }

    if let Some(project) = args.project {
        return run_project_with_options(
            &project,
            args.parity,
            args.emit_fixtures,
            args.pack,
            args.layout,
        );
    }

    if let Some(csv) = args.type_table_csv {
        let output = args.type_table_output.ok_or_else(|| {
            anyhow::anyhow!("--type-table-output is required with --type-table-csv")
        })?;
        let table = builder::load_type_table(&csv)
            .with_context(|| format!("Loading type table from {}", csv.display()))?;
        let bytes = builder::build_type_table(&table);
        builder::write_type_table(&output, &table)
            .with_context(|| format!("Writing type table to {}", output.display()))?;
        if args.parity {
            let rust_command = format!(
                "cgfx-tools --type-table-csv {} --output {} --parity",
                csv.display(),
                output.display()
            );
            builder::verify_type_table_python_parity(&csv, &bytes, &rust_command)?;
        }
        return Ok(());
    }

    if let Some(csv) = args.consumables_csv {
        let output = args.consumables_output.ok_or_else(|| {
            anyhow::anyhow!("--consumables-output is required with --consumables-csv")
        })?;
        let table = builder::load_consumables_csv(&csv)
            .with_context(|| format!("Loading consumables from {}", csv.display()))?;
        builder::write_consumables(&output, &table)
            .with_context(|| format!("Writing consumables to {}", output.display()))?;
        return Ok(());
    }

    if let Some(csv) = args.opponents_csv {
        let output = args.opponents_output.ok_or_else(|| {
            anyhow::anyhow!("--opponents-output is required with --opponents-csv")
        })?;
        let seeds = builder::load_opponents_csv(&csv)
            .with_context(|| format!("Loading opponents from {}", csv.display()))?;
        let bytes = builder::build_opponents(&seeds);
        std::fs::create_dir_all(&output)
            .with_context(|| format!("Creating {}", output.display()))?;
        let output_path = output.join("opponents.bin");
        std::fs::write(&output_path, &bytes)
            .with_context(|| format!("Writing {}", output_path.display()))?;
        if args.parity {
            let rust_command = format!(
                "cgfx-tools --opponents-csv {} --opponents-output {} --parity",
                csv.display(),
                output.display()
            );
            builder::verify_opponents_python_parity(&csv, &bytes, &rust_command)?;
        }
        return Ok(());
    }

    if let Some(csv) = args.arena_csv {
        let output = args
            .arena_output
            .ok_or_else(|| anyhow::anyhow!("--arena-output is required with --arena-csv"))?;
        let entries = arena::parse_arena_csv(&csv)
            .with_context(|| format!("Loading arena from {}", csv.display()))?;
        let bytes = arena::build_arena(&entries);
        std::fs::create_dir_all(&output)
            .with_context(|| format!("Creating {}", output.display()))?;
        let output_path = output.join("arena.txt");
        std::fs::write(&output_path, &bytes)
            .with_context(|| format!("Writing {}", output_path.display()))?;
        if args.parity {
            let root = csv
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| anyhow::anyhow!("finding project root from {}", csv.display()))?;
            let rust_command = format!(
                "cgfx-tools --arena-csv {} --arena-output {} --parity",
                csv.display(),
                output.display()
            );
            arena::verify_arena_parity(&csv, root.join("tools/arena.py"), &rust_command)?;
        }
        return Ok(());
    }

    if let Some(config) = args.bw_placeholders {
        let archive_dir = args
            .archive_dir
            .expect("clap requires --archive-dir with --bw-placeholders");
        let stats = fxpack::bw_placeholders::convert(&config, &archive_dir)?;
        println!(
            "bw-placeholders: archived {}, converted {}.",
            stats.archived, stats.converted
        );
        return Ok(());
    }

    if let Some(config) = args.sprite_config {
        return fxpack::sprite_source::emit(&config);
    }

    let input = args.input.expect("clap requires input without --project");
    let output = args.output.expect("clap requires output without --project");

    // 1. ── Parse ──────────────────────────────────────────────────────
    let json =
        std::fs::read_to_string(&input).with_context(|| format!("Reading {}", input.display()))?;
    let raw_project = parser::load(&json).with_context(|| "Parsing input JSON")?;

    // 2. ── Process ────────────────────────────────────────────────────
    let processed =
        processor::run(&raw_project).with_context(|| "Processing / assembling VM scripts")?;

    // 3. ── Write outputs ──────────────────────────────────────────────
    std::fs::create_dir_all(&output).with_context(|| format!("Creating {}", output.display()))?;

    println!("Writing C headers");
    writer::c::emit(&processed, &output).with_context(|| "Writing C artifacts")?;
    println!("Writing binary artifacts");
    writer::bin::emit(&processed, &output).with_context(|| "Writing binary artifacts")?;

    Ok(())
}

/// Compiles a complete data project described by a config file.
pub fn run_project(path: impl AsRef<Path>) -> anyhow::Result<()> {
    run_project_with_options(path, false, false, false, None)
}

fn run_project_with_options(
    path: impl AsRef<Path>,
    parity: bool,
    emit_fixtures: bool,
    pack: bool,
    layout: Option<PathBuf>,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let project_root = path.parent().unwrap_or_else(|| Path::new("."));
    let _ = parity;
    let cfg = config::Config::load_from(path)?;
    let store = store::Store::new(&cfg);

    let creatures = store
        .load_creatures()
        .with_context(|| "Loading creatures")?;
    builder::write_creatures(&cfg, &creatures).with_context(|| "Writing creature artifacts")?;

    let opponents_csv = project_root.join("data/opponents.csv");
    let opponents = builder::load_opponents_csv(&opponents_csv)
        .with_context(|| format!("Loading opponents from {}", opponents_csv.display()))?;
    let battle_presets = cfg
        .battle_presets
        .as_deref()
        .map(|path| battle_presets::load(path, &creatures, &opponents))
        .transpose()
        .with_context(|| "Loading battle presets")?;
    let arena_demo = cfg
        .arena_demo
        .as_deref()
        .map(|path| arena_demo::load(path, &creatures, &opponents))
        .transpose()
        .with_context(|| "Loading arena demo catalog")?;
    let duel = cfg
        .duel
        .as_deref()
        .map(|path| duel::load(path, &creatures, &opponents))
        .transpose()
        .with_context(|| "Loading duel data")?;
    let rewards = cfg
        .beta_area
        .as_deref()
        .map(|path| {
            let consumables_csv = project_root.join("data/consumables.csv");
            let consumables = builder::load_consumables_csv(&consumables_csv).map_err(|e| {
                crate::builder::BuildError(format!(
                    "loading consumables from {}: {e}",
                    consumables_csv.display()
                ))
            })?;
            rewards::load(path, &creatures, &consumables)
        })
        .transpose()
        .with_context(|| "Loading first-area reward mappings")?;
    if let Some(rewards) = &rewards {
        rewards::write_reward_descriptor(Path::new(&cfg.output_dir), rewards)
            .with_context(|| "Emitting first-area reward mapping")?;
    }
    if let Some(catalog) = &arena_demo {
        arena_demo::emit(
            catalog,
            Path::new(&cfg.output_dir),
            if emit_fixtures {
                cfg.fixture_dir.as_deref().map(Path::new)
            } else {
                None
            },
        )
        .with_context(|| "Emitting arena demo catalog")?;
    }
    if let Some(catalog) = &duel {
        duel::emit(
            catalog,
            Path::new(&cfg.output_dir),
            if emit_fixtures {
                cfg.fixture_dir.as_deref().map(Path::new)
            } else {
                None
            },
        )
        .with_context(|| "Emitting duel data")?;
    }
    builder::write_fx_descriptors(&cfg.output_dir, &creatures, &opponents)
        .with_context(|| "Writing FX text descriptors")?;

    // `fxdata.txt` includes creatureData from its sibling data directory.
    // Keep the generated copy in output_dir for tool consumers and update the
    // canonical pack input when this is an FX project layout.
    let canonical_creature = project_root.join("fxdata/data/creatureData.txt");
    if canonical_creature.parent().is_some_and(Path::exists) {
        let descriptor = builder::build_fx_creature_descriptor(&creatures)?;
        std::fs::write(&canonical_creature, descriptor)
            .with_context(|| format!("Writing {}", canonical_creature.display()))?;
    }

    let map = store.load_map().with_context(|| "Loading map")?;
    builder::write_map(&cfg, &map).with_context(|| "Writing map artifacts")?;

    if emit_fixtures {
        let fixture_dir = cfg.fixture_dir.as_deref().ok_or_else(|| {
            anyhow::anyhow!("--emit-fixtures requires fixture_dir in {}", path.display())
        })?;
        let arena_csv = project_root.join("data/arena.csv");
        let arena = arena::parse_arena_csv(&arena_csv)
            .with_context(|| format!("Loading arena from {}", arena_csv.display()))?;
        let map_artifacts = builder::build_map(&map)?;
        let processed = fixtures::FixtureProject {
            creatures: &creatures,
            opponents: &opponents,
            arena: &arena,
            map: &map,
            scripts: &map_artifacts.scripts_bin,
            rewards: rewards.as_ref(),
        };
        fixtures::emit_fixtures(&processed, fixture_dir)
            .with_context(|| format!("Writing fixtures to {fixture_dir}"))?;
        if let Some(presets) = &battle_presets {
            battle_presets::emit_header(
                presets,
                Path::new(fixture_dir).join("battle_preset_data.hpp"),
            )
            .with_context(|| "Writing battle preset fixture")?;
        }
    }

    let creature_source = Path::new(&cfg.creatures_json);
    let map_source = Path::new(&cfg.map_json);
    let output_dir = Path::new(&cfg.output_dir);
    let mut artifact_inputs = vec![
        manifest::ArtifactInput::in_output(output_dir, "creatureData.bin", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "creatureData.hpp", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "creatureData.txt", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "attackMask.txt", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "evolution.txt", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "opponents.txt", &opponents_csv),
        manifest::ArtifactInput::in_output(output_dir, "raw_map.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "map.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "scripts.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "text.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "opcodes.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "flags.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "flag_bit_array.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "flag_bit_array.cpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "locations.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "scripts.hpp", map_source),
    ];
    if let (Some(source), Some(catalog)) = (cfg.arena_demo.as_deref(), arena_demo.as_ref()) {
        let source = Path::new(source);
        artifact_inputs.push(manifest::ArtifactInput::in_output(
            output_dir,
            "arena_demo.txt",
            source,
        ));
        artifact_inputs.push(manifest::ArtifactInput::in_output(
            output_dir,
            "arena_demo_ids.hpp",
            source,
        ));
        artifact_inputs.push(manifest::ArtifactInput::external(
            &catalog.label_font,
            source,
        ));
    }
    if let Some(source) = cfg.duel.as_deref().filter(|_| duel.is_some()) {
        let source = Path::new(source);
        artifact_inputs.push(manifest::ArtifactInput::in_output(
            output_dir,
            "duel.txt",
            source,
        ));
        artifact_inputs.push(manifest::ArtifactInput::in_output(
            output_dir,
            "duel_ids.hpp",
            source,
        ));
    }
    if let Some(source) = cfg.beta_area.as_deref().filter(|_| rewards.is_some()) {
        artifact_inputs.push(manifest::ArtifactInput::in_output(
            output_dir,
            "rewards.txt",
            Path::new(source),
        ));
    }
    if canonical_creature.is_file() {
        let canonical = canonical_creature
            .canonicalize()
            .with_context(|| format!("Resolving {}", canonical_creature.display()))?;
        artifact_inputs.push(manifest::ArtifactInput::external(
            &canonical,
            creature_source,
        ));
    }
    if pack {
        let layout_path = layout.unwrap_or_else(|| project_root.join("fxlayout.toml"));
        let spec = fxpack::spec::LayoutSpec::load(&layout_path)
            .with_context(|| format!("loading layout spec {}", layout_path.display()))?;
        let keys = spec.builder_keys();
        let mut builder_list = Vec::new();
        for &key in &keys {
            let bin_name = match key {
                "creature_data" => "creatureData.bin",
                "opponents" => "opponents.bin",
                "type_table" => "typetable.bin",
                "map_data" => "map.bin",
                "raw_map" => "raw_map.bin",
                "text" => "text.bin",
                "scripts" => "scripts.bin",
                "tool_version" => "tool_version.bin",
                other => other,
            };
            let builder_path = output_dir.join(bin_name);
            let bytes = std::fs::read(&builder_path).with_context(|| {
                format!("reading builder {key:?} from {}", builder_path.display())
            })?;
            builder_list.push(fxpack::pack::BuilderBytes {
                key,
                bytes,
                namespace: None,
            });
        }
        let dist_dir = project_root.join("dist");
        let header_path = project_root.join("src/fxdata.h");
        let outcome = fxpack::pack::pack(&spec, &builder_list, &dist_dir, &header_path)
            .with_context(|| "packing FX image")?;
        println!("Packed FX image: {}", outcome.artifacts.dev_bin.display());
    }

    manifest::emit(output_dir, &artifact_inputs)
        .with_context(|| format!("Writing manifest in {}", cfg.output_dir))?;

    Ok(())
}
