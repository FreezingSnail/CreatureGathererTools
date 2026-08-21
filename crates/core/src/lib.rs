pub mod arena;
pub mod builder;
pub mod cli;
pub mod config;
pub mod model;
pub mod parser;
pub mod processor;
pub mod script;
pub mod store;
pub mod tiled;
pub mod writer;
use anyhow::Context;
use clap::Parser;
use std::path::Path;

pub fn run() -> anyhow::Result<()> {
    let args = cli::Cli::parse();

    if let Some(project) = args.project {
        return run_project(&project);
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
    let cfg = config::Config::load_from(path)?;
    let store = store::Store::new(&cfg);

    let creatures = store
        .load_creatures()
        .with_context(|| "Loading creatures")?;
    builder::write_creatures(&cfg, &creatures).with_context(|| "Writing creature artifacts")?;

    let moves = store.load_moves().with_context(|| "Loading moves")?;
    builder::write_moves(&cfg, &moves).with_context(|| "Writing move artifacts")?;

    let map = store.load_map().with_context(|| "Loading map")?;
    builder::write_map(&cfg, &map).with_context(|| "Writing map artifacts")?;

    Ok(())
}
