pub mod cli;
pub mod model;
pub mod parser;
pub mod processor;
pub mod writer;

use anyhow::Context;
use clap::Parser;

pub fn run() -> anyhow::Result<()> {
    let args = cli::Cli::parse();

    // 1. ── Parse ──────────────────────────────────────────────────────
    let json = std::fs::read_to_string(&args.input)
        .with_context(|| format!("Reading {}", args.input.display()))?;
    let raw_project = parser::load_from_json(&json).with_context(|| "Parsing input JSON")?;

    // 2. ── Process ────────────────────────────────────────────────────
    let processed =
        processor::run(&raw_project).with_context(|| "Processing / assembling VM scripts")?;

    // 3. ── Write outputs ──────────────────────────────────────────────
    std::fs::create_dir_all(&args.output)
        .with_context(|| format!("Creating {}", args.output.display()))?;

    writer::c::emit(&processed, &args.output).with_context(|| "Writing C artifacts")?;
    writer::bin::emit(&processed, &args.output).with_context(|| "Writing binary artifacts")?;

    Ok(())
}
