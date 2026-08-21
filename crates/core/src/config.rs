//! Config loading, ported from creatureTools internal/config/config.go.
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "creatures_json")]
    pub creatures_json: String,
    #[serde(rename = "moves_json")]
    pub moves_json: String,
    #[serde(rename = "map_json")]
    pub map_json: String,
    #[serde(rename = "tileset_png")]
    pub tileset_png: String, // legacy / map layer sheet
    #[serde(rename = "map_tileset_png")]
    pub map_tileset_png: String, // map layer sprite sheet
    #[serde(rename = "npc_tileset_png")]
    pub npc_tileset_png: String, // npc layer sprite sheet
    #[serde(rename = "output_dir")]
    pub output_dir: String,
    #[serde(default, rename = "fixture_dir")]
    pub fixture_dir: Option<String>,
}

impl Config {
    /// Mirrors Go `config.Load()`: reads "creature-tools.json" from the
    /// current working directory.
    pub fn load() -> anyhow::Result<Config> {
        Self::load_from("creature-tools.json")
    }

    /// Loads a config file and resolves every relative path against its parent
    /// directory, independent of the caller's current working directory.
    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Config> {
        let path = path.as_ref();
        let data =
            std::fs::read(path).with_context(|| format!("reading config {}", path.display()))?;
        let mut cfg: Config = serde_json::from_slice(&data)
            .map_err(|e| anyhow::anyhow!("parsing config {}: {e}", path.display()))?;
        cfg.resolve_relative_paths(path.parent().unwrap_or_else(|| Path::new(".")));
        Ok(cfg)
    }

    fn resolve_relative_paths(&mut self, config_dir: &Path) {
        resolve_relative_path(&mut self.creatures_json, config_dir);
        resolve_relative_path(&mut self.moves_json, config_dir);
        resolve_relative_path(&mut self.map_json, config_dir);
        resolve_relative_path(&mut self.tileset_png, config_dir);
        resolve_relative_path(&mut self.map_tileset_png, config_dir);
        resolve_relative_path(&mut self.npc_tileset_png, config_dir);
        resolve_relative_path(&mut self.output_dir, config_dir);
        if let Some(fixture_dir) = &mut self.fixture_dir {
            resolve_relative_path(fixture_dir, config_dir);
        }
    }
}

fn resolve_relative_path(path: &mut String, config_dir: &Path) {
    if Path::new(path).is_relative() {
        *path = config_dir.join(&*path).to_string_lossy().into_owned();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/creature-tools.json");
    const MISSING_FIELD: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/missing-field-project.json"
    );

    #[test]
    fn load_valid_resolves_relative_paths_and_fixture_dir() {
        let cfg = Config::load_from(FIXTURE).expect("fixture must parse");
        let base = Path::new(FIXTURE).parent().expect("fixture parent");
        assert_eq!(
            cfg.creatures_json,
            base.join("creatures.json").to_string_lossy()
        );
        assert_eq!(cfg.moves_json, base.join("moves.json").to_string_lossy());
        assert_eq!(cfg.map_json, base.join("map.json").to_string_lossy());
        assert_eq!(cfg.tileset_png, base.join("tileset.png").to_string_lossy());
        assert_eq!(
            cfg.map_tileset_png,
            base.join("map_tileset.png").to_string_lossy()
        );
        assert_eq!(
            cfg.npc_tileset_png,
            base.join("npc_tileset.png").to_string_lossy()
        );
        assert_eq!(
            cfg.output_dir,
            base.join("builder_out/project").to_string_lossy()
        );
        assert_eq!(
            cfg.fixture_dir,
            Some(base.join("fixtures").to_string_lossy().into_owned())
        );
    }

    #[test]
    fn load_missing_errors_name_config_file() {
        let path = "/nonexistent/creature-tools.json";
        let err = Config::load_from(path).expect_err("missing config must fail");
        assert!(err.to_string().contains(path));
    }

    #[test]
    fn load_malformed_errors_name_config_file() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/bad-creature-tools.json"
        );
        let err = Config::load_from(path).expect_err("malformed config must fail");
        assert!(err.to_string().contains(path));
    }

    #[test]
    fn load_missing_field_errors_name_config_file_and_field() {
        let err = Config::load_from(MISSING_FIELD).expect_err("incomplete config must fail");
        let message = err.to_string();
        assert!(message.contains(MISSING_FIELD));
        assert!(message.contains("creatures_json"));
    }
}
