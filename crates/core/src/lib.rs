pub mod arena;
pub mod builder;
pub mod cli;
pub mod config;
pub mod fixtures;
pub mod fxpack;
pub mod manifest;
pub mod model;
pub mod parser;
pub mod processor;
pub mod script;
pub mod store;
pub mod tiled;
pub mod writer;
use anyhow::Context;
use clap::Parser;
use std::path::{Path, PathBuf};

pub fn run() -> anyhow::Result<()> {
    let args = cli::Cli::parse();

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
    let reference = parity
        .then(|| builder::capture_project_fx_descriptor_reference(project_root))
        .transpose()?;
    let cfg = config::Config::load_from(path)?;
    let store = store::Store::new(&cfg);

    let creatures = store
        .load_creatures()
        .with_context(|| "Loading creatures")?;
    builder::write_creatures(&cfg, &creatures).with_context(|| "Writing creature artifacts")?;

    let moves = store.load_moves().with_context(|| "Loading moves")?;
    builder::write_moves(&cfg, &moves).with_context(|| "Writing move artifacts")?;

    let opponents_csv = project_root.join("data/opponents.csv");
    let opponents = builder::load_opponents_csv(&opponents_csv)
        .with_context(|| format!("Loading opponents from {}", opponents_csv.display()))?;
    builder::write_fx_descriptors(&cfg.output_dir, &creatures, &moves, &opponents)
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

    if let Some(reference) = reference {
        let output = Path::new(&cfg.output_dir);
        let actual = builder::FxDescriptorReference {
            opponents: std::fs::read(output.join("opponents.txt"))?,
            creatures: std::fs::read(output.join("creatureData.txt"))?,
            moves: std::fs::read(output.join("movedata.txt"))?,
        };
        builder::verify_fx_descriptor_parity(
            &reference,
            &actual,
            "python3 tools/data_converters/{opponent_data,creature_data}.py",
            &format!("cgfx-tools --project {} --parity", path.display()),
        )?;
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
        let processed = fixtures::FixtureProject {
            creatures: &creatures,
            moves: &moves,
            opponents: &opponents,
            arena: &arena,
        };
        fixtures::emit_fixtures(&processed, fixture_dir)
            .with_context(|| format!("Writing fixtures to {fixture_dir}"))?;
    }

    let creature_source = Path::new(&cfg.creatures_json);
    let move_source = Path::new(&cfg.moves_json);
    let map_source = Path::new(&cfg.map_json);
    let output_dir = Path::new(&cfg.output_dir);
    let mut artifact_inputs = vec![
        manifest::ArtifactInput::in_output(output_dir, "creatureData.bin", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "creatureData.hpp", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "moves.bin", move_source),
        manifest::ArtifactInput::in_output(output_dir, "moves.hpp", move_source),
        manifest::ArtifactInput::in_output(output_dir, "creatureData.txt", creature_source),
        manifest::ArtifactInput::in_output(output_dir, "movedata.txt", move_source),
        manifest::ArtifactInput::in_output(output_dir, "opponents.txt", &opponents_csv),
        manifest::ArtifactInput::in_output(output_dir, "raw_map.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "scripts.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "text.bin", map_source),
        manifest::ArtifactInput::in_output(output_dir, "opcodes.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "flags.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "flag_bit_array.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "flag_bit_array.cpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "locations.hpp", map_source),
        manifest::ArtifactInput::in_output(output_dir, "scripts.hpp", map_source),
    ];
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
                "move_table" => "moves.bin",
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
