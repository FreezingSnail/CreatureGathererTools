use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "cgfx-tools",
    author,
    version,
    about,
    subcommand_negates_reqs = true,
    args_conflicts_with_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    /// Project config file. Relative config values resolve from this file's directory.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = [
            "input",
            "output",
            "opponents_csv",
            "opponents_output",
            "arena_csv",
            "arena_output",
            "type_table_csv",
            "type_table_output",
            "consumables_csv",
            "consumables_output",
            "sprite_config",
            "bw_placeholders",
            "assert_layout_equivalent"
        ]
    )]
    pub project: Option<PathBuf>,
    /// Assert two complete layout specs produce identical fields and packed bytes.
    #[arg(
        long,
        value_names = ["OLD", "NEW"],
        num_args = 2,
        conflicts_with_all = [
            "project",
            "input",
            "output",
            "opponents_csv",
            "opponents_output",
            "arena_csv",
            "arena_output",
            "type_table_csv",
            "type_table_output",
            "consumables_csv",
            "consumables_output",
            "sprite_config",
            "bw_placeholders"
        ]
    )]
    pub assert_layout_equivalent: Option<Vec<PathBuf>>,
    /// Input .json map / project file (legacy script compiler mode)
    #[arg(required_unless_present_any = [
        "project",
        "opponents_csv",
        "arena_csv",
        "type_table_csv",
        "consumables_csv",
        "sprite_config",
        "bw_placeholders",
        "assert_layout_equivalent"
    ])]
    pub input: Option<PathBuf>,
    /// Output directory for legacy compilation, arena source, or `typetable.bin`.
    #[arg(required_unless_present_any = [
        "project",
        "opponents_csv",
        "arena_csv",
        "type_table_csv",
        "consumables_csv",
        "sprite_config",
        "bw_placeholders",
        "assert_layout_equivalent"
    ])]
    pub output: Option<PathBuf>,
    /// Output directory for Rust-built `opponents.bin`.
    #[arg(long, value_name = "DIR", requires = "opponents_csv")]
    pub opponents_output: Option<PathBuf>,
    /// Output directory for Rust-built arena `teamList` source.
    #[arg(long, value_name = "DIR", requires = "arena_csv")]
    pub arena_output: Option<PathBuf>,
    /// Output directory for Rust-built `typetable.bin`.
    #[arg(long, value_name = "DIR", requires = "type_table_csv")]
    pub type_table_output: Option<PathBuf>,
    /// Output directory for Rust-built `consumables.bin`.
    #[arg(long, value_name = "DIR", requires = "consumables_csv")]
    pub consumables_output: Option<PathBuf>,
    /// Build `opponents.bin` from the canonical `data/opponents.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "arena_csv", "type_table_csv", "consumables_csv"])]
    pub opponents_csv: Option<PathBuf>,
    /// Build arena `teamList` source bytes from the canonical `data/arena.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "opponents_csv", "type_table_csv", "consumables_csv"])]
    pub arena_csv: Option<PathBuf>,
    /// Build `typetable.bin` from the canonical `data/typetable.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "opponents_csv", "arena_csv", "consumables_csv"])]
    pub type_table_csv: Option<PathBuf>,
    /// Build `consumables.bin` from the canonical `data/consumables.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "opponents_csv", "arena_csv", "type_table_csv"])]
    pub consumables_csv: Option<PathBuf>,
    /// Emit legacy-compatible sprite C arrays from a native TOML source config.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["input", "output", "project", "opponents_csv", "arena_csv", "type_table_csv", "consumables_csv", "consumables_output", "bw_placeholders"])]
    pub sprite_config: Option<PathBuf>,
    /// Rewrite referenced PNGs as black/white RGBA8 placeholders after archiving originals.
    #[arg(
        long,
        value_name = "FILE",
        requires = "archive_dir",
        conflicts_with_all = [
            "project",
            "input",
            "output",
            "opponents_csv",
            "opponents_output",
            "arena_csv",
            "arena_output",
            "type_table_csv",
            "type_table_output",
            "consumables_csv",
            "consumables_output",
            "sprite_config",
            "assert_layout_equivalent",
            "parity",
            "emit_fixtures",
            "pack",
            "layout"
        ]
    )]
    pub bw_placeholders: Option<PathBuf>,
    /// Directory receiving immutable byte-for-byte copies before placeholder conversion.
    #[arg(
        long,
        value_name = "DIR",
        requires = "bw_placeholders",
        conflicts_with_all = [
            "project",
            "input",
            "output",
            "opponents_csv",
            "opponents_output",
            "arena_csv",
            "arena_output",
            "type_table_csv",
            "type_table_output",
            "consumables_csv",
            "consumables_output",
            "sprite_config",
            "assert_layout_equivalent",
            "parity",
            "emit_fixtures",
            "pack",
            "layout"
        ]
    )]
    pub archive_dir: Option<PathBuf>,
    /// Run the applicable legacy Python generators and byte-compare outputs.
    #[arg(long)]
    pub parity: bool,
    /// Emit model-derived FX device-test fixture headers from the project configuration.
    #[arg(long, requires = "project")]
    pub emit_fixtures: bool,
    /// Pack the FX image described by fxlayout.toml.
    #[arg(long, requires = "project")]
    pub pack: bool,
    /// Path to the layout spec (defaults to fxlayout.toml beside the project config).
    #[arg(long, value_name = "FILE", requires = "pack")]
    pub layout: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Generate WFC tile hunks from an authored map project.
    #[command(subcommand)]
    Wfc(WfcCommand),
}

#[derive(Subcommand, Debug)]
pub enum WfcCommand {
    /// Generate one hunk from a named or numeric recipe.
    Generate(WfcGenerate),
}

#[derive(Args, Debug)]
pub struct WfcGenerate {
    /// Map project JSON containing its optional `wfc` project.
    #[arg(long, value_name = "MAP")]
    pub map: PathBuf,
    /// Recipe ID or exact recipe name.
    #[arg(long, value_name = "ID|NAME")]
    pub recipe: String,
    /// Override the recipe seed.
    #[arg(long)]
    pub seed: Option<u64>,
    /// Override recipe dimensions in solver slots, as `WxH`.
    #[arg(long, value_name = "WxH", value_parser = parse_slot_size)]
    pub size: Option<(u32, u32)>,
    /// Write a standalone generated-hunk JSON file.
    #[arg(long, value_name = "FILE", conflicts_with = "in_place")]
    pub out: Option<PathBuf>,
    /// Append the generated hunk to the input map's stamp library.
    #[arg(long, conflicts_with = "out")]
    pub in_place: bool,
    /// Print the solved slot grid and counts without writing files.
    #[arg(long)]
    pub dry_run: bool,
    /// Name for an in-place stamp.
    #[arg(long, value_name = "NAME", requires = "in_place")]
    pub stamp_name: Option<String>,
}

fn parse_slot_size(value: &str) -> Result<(u32, u32), String> {
    let Some((width, height)) = value.split_once('x') else {
        return Err("size must use WxH, for example 4x3".to_string());
    };
    let width = width
        .parse::<u32>()
        .map_err(|_| "size width must be an unsigned integer".to_string())?;
    let height = height
        .parse::<u32>()
        .map_err(|_| "size height must be an unsigned integer".to_string())?;
    if width == 0 || height == 0 {
        return Err("size dimensions must be greater than zero".to_string());
    }
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn wfc_generate_accepts_all_generation_options() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "wfc",
            "generate",
            "--map",
            "map.json",
            "--recipe",
            "alternating",
            "--seed",
            "8",
            "--size",
            "5x4",
            "--out",
            "hunk.json",
        ])
        .expect("WFC generation invocation parses");
        let Some(Command::Wfc(WfcCommand::Generate(args))) = cli.command else {
            panic!("expected WFC generate command");
        };
        assert_eq!(args.map, PathBuf::from("map.json"));
        assert_eq!(args.recipe, "alternating");
        assert_eq!(args.seed, Some(8));
        assert_eq!(args.size, Some((5, 4)));
        assert_eq!(args.out, Some(PathBuf::from("hunk.json")));
    }

    #[test]
    fn positional_legacy_invocation_still_parses() {
        let cli = Cli::try_parse_from(["cgfx-tools", "input.json", "output"])
            .expect("legacy invocation parses");
        assert_eq!(cli.input, Some(PathBuf::from("input.json")));
        assert_eq!(cli.output, Some(PathBuf::from("output")));
        assert!(cli.command.is_none());
    }

    #[test]
    fn project_option_accepts_explicit_config_path() {
        let cli = Cli::try_parse_from(["cgfx-tools", "--project", "cgfx-project.json"])
            .expect("project invocation parses");
        assert_eq!(cli.project, Some(PathBuf::from("cgfx-project.json")));
        assert!(cli.input.is_none());
        assert!(cli.output.is_none());
    }

    #[test]
    fn project_mode_accepts_parity() {
        let cli = Cli::try_parse_from(["cgfx-tools", "--project", "cgfx-project.json", "--parity"])
            .expect("project parity invocation parses");
        assert_eq!(cli.project, Some(PathBuf::from("cgfx-project.json")));
        assert!(cli.parity);
    }

    #[test]
    fn opponent_mode_accepts_csv_output_and_parity() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--opponents-csv",
            "data/opponents.csv",
            "--opponents-output",
            "out",
            "--parity",
        ])
        .expect("opponent invocation parses");
        assert_eq!(cli.opponents_csv, Some(PathBuf::from("data/opponents.csv")));
        assert_eq!(cli.opponents_output, Some(PathBuf::from("out")));
        assert!(cli.parity);
    }

    #[test]
    fn type_table_mode_accepts_csv_output_and_parity() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--type-table-csv",
            "data/typetable.csv",
            "--type-table-output",
            "out",
            "--parity",
        ])
        .expect("type-table invocation parses");
        assert_eq!(
            cli.type_table_csv,
            Some(PathBuf::from("data/typetable.csv"))
        );
        assert_eq!(cli.type_table_output, Some(PathBuf::from("out")));
        assert!(cli.parity);
    }

    #[test]
    fn consumables_mode_accepts_csv_and_output() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--consumables-csv",
            "data/consumables.csv",
            "--consumables-output",
            "out",
        ])
        .expect("consumables invocation parses");
        assert_eq!(
            cli.consumables_csv,
            Some(PathBuf::from("data/consumables.csv"))
        );
        assert_eq!(cli.consumables_output, Some(PathBuf::from("out")));
        assert!(Cli::try_parse_from(["cgfx-tools", "--consumables-output", "out"]).is_err());
    }

    #[test]
    fn project_mode_accepts_emit_fixtures() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--project",
            "cgfx-project.json",
            "--emit-fixtures",
        ])
        .expect("fixture project invocation parses");
        assert_eq!(cli.project, Some(PathBuf::from("cgfx-project.json")));
        assert!(cli.emit_fixtures);
    }

    #[test]
    fn project_mode_accepts_pack_and_layout() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--project",
            "cgfx-project.json",
            "--pack",
            "--layout",
            "custom-layout.toml",
        ])
        .expect("pack project invocation parses");
        assert!(cli.pack);
        assert_eq!(cli.layout, Some(PathBuf::from("custom-layout.toml")));
    }

    #[test]
    fn layout_equivalence_mode_accepts_two_layouts_without_project() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--assert-layout-equivalent",
            "old.toml",
            "new.toml",
        ])
        .expect("equivalence invocation parses");
        assert_eq!(
            cli.assert_layout_equivalent,
            Some(vec![PathBuf::from("old.toml"), PathBuf::from("new.toml")])
        );
        assert!(cli.input.is_none());
        assert!(cli.output.is_none());
    }

    #[test]
    fn pack_requires_project() {
        assert!(Cli::try_parse_from(["cgfx-tools", "--pack"]).is_err());
    }

    #[test]
    fn layout_requires_pack() {
        assert!(
            Cli::try_parse_from([
                "cgfx-tools",
                "--project",
                "cgfx-project.json",
                "--layout",
                "custom-layout.toml",
            ])
            .is_err()
        );
    }

    #[test]
    fn version_banner_uses_stable_binary_name() {
        assert_eq!(
            Cli::command().render_version().to_string(),
            format!("cgfx-tools {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn sprite_config_mode_requires_no_legacy_io_arguments() {
        let cli = Cli::try_parse_from(["cgfx-tools", "--sprite-config", "fxsprites.toml"])
            .expect("sprite config invocation parses");
        assert_eq!(cli.sprite_config, Some(PathBuf::from("fxsprites.toml")));
        assert!(cli.input.is_none());
        assert!(cli.output.is_none());
    }

    #[test]
    fn bw_placeholder_mode_requires_archive_dir() {
        let cli = Cli::try_parse_from([
            "cgfx-tools",
            "--bw-placeholders",
            "fxsprites.toml",
            "--archive-dir",
            "archive",
        ])
        .expect("bw placeholder invocation parses");
        assert_eq!(cli.bw_placeholders, Some(PathBuf::from("fxsprites.toml")));
        assert_eq!(cli.archive_dir, Some(PathBuf::from("archive")));
        assert!(
            Cli::try_parse_from(["cgfx-tools", "--bw-placeholders", "fxsprites.toml"]).is_err()
        );
        assert!(Cli::try_parse_from(["cgfx-tools", "--archive-dir", "archive"]).is_err());
    }

    #[test]
    fn bw_placeholder_mode_conflicts_with_other_modes() {
        assert!(
            Cli::try_parse_from([
                "cgfx-tools",
                "--bw-placeholders",
                "fxsprites.toml",
                "--archive-dir",
                "archive",
                "--sprite-config",
                "other.toml",
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "cgfx-tools",
                "--bw-placeholders",
                "fxsprites.toml",
                "--archive-dir",
                "archive",
                "input.json",
                "output",
            ])
            .is_err()
        );
    }
}
