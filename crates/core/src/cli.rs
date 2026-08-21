use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cgfx-tools", author, version, about)]
pub struct Cli {
    /// Project config file. Relative config values resolve from this file's directory.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["input", "output"])]
    pub project: Option<PathBuf>,
    /// Input .json map / project file (legacy script compiler mode)
    #[arg(required_unless_present = "project")]
    pub input: Option<PathBuf>,
    /// Output directory (legacy script compiler mode)
    #[arg(required_unless_present = "project")]
    pub output: Option<PathBuf>,
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
    fn version_banner_uses_stable_binary_name() {
        assert_eq!(
            Cli::command().render_version().to_string(),
            format!("cgfx-tools {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
}
