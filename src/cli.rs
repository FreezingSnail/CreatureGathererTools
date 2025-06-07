use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub struct Cli {
    /// Input .json map / project file
    pub input: PathBuf,
    /// Output directory
    pub output: PathBuf,
}