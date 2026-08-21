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
            "parity"
        ]
    )]
    pub project: Option<PathBuf>,
    /// Input .json map / project file (legacy script compiler mode)
    #[arg(required_unless_present_any = [
        "project",
        "opponents_csv",
        "arena_csv",
        "type_table_csv"
    ])]
    pub input: Option<PathBuf>,
    /// Output directory for legacy compilation, arena source, or `typetable.bin`.
    #[arg(required_unless_present_any = [
        "project",
        "opponents_csv",
        "arena_csv",
        "type_table_csv"
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
    /// Run the selected legacy Python generator and byte-compare its output.
    /// Requires a CSV builder mode; validated by the selected mode.
    #[arg(long)]
    pub parity: bool,
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
    fn version_banner_uses_stable_binary_name() {
        assert_eq!(
            Cli::command().render_version().to_string(),
            format!("cgfx-tools {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
