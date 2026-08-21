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
