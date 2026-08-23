use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cgfx-tools", author, version, about)]
pub struct Cli {
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
            "sprite_config"
        ]
    )]
    pub project: Option<PathBuf>,
    /// Input .json map / project file (legacy script compiler mode)
    #[arg(required_unless_present_any = [
        "project",
        "opponents_csv",
        "arena_csv",
        "type_table_csv",
        "sprite_config"
    ])]
    pub input: Option<PathBuf>,
    /// Output directory for legacy compilation, arena source, or `typetable.bin`.
    #[arg(required_unless_present_any = [
        "project",
        "opponents_csv",
        "arena_csv",
        "type_table_csv",
        "sprite_config"
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
    /// Build `opponents.bin` from the canonical `data/opponents.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "arena_csv", "type_table_csv"])]
    pub opponents_csv: Option<PathBuf>,
    /// Build arena `teamList` source bytes from the canonical `data/arena.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "opponents_csv", "type_table_csv"])]
    pub arena_csv: Option<PathBuf>,
    /// Build `typetable.bin` from the canonical `data/typetable.csv` source.
    #[arg(long, value_name = "CSV", conflicts_with_all = ["input", "opponents_csv", "arena_csv"])]
    pub type_table_csv: Option<PathBuf>,
    /// Emit legacy-compatible sprite C arrays from a native TOML source config.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["input", "output", "project", "opponents_csv", "arena_csv", "type_table_csv"])]
    pub sprite_config: Option<PathBuf>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

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
}
