//! Config loading, ported from creatureTools internal/config/config.go.
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
}

impl Config {
    /// Mirrors Go `config.Load()`: reads "creature-tools.json" from the
    /// current working directory.
    pub fn load() -> anyhow::Result<Config> {
        Self::load_from("creature-tools.json")
    }

    /// Same behavior as `load()` but with an explicit path (used by tests
    /// and by embedders that resolve config paths themselves).
    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Config> {
        let data = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("reading creature-tools.json: {e}"))?;
        let cfg = serde_json::from_slice(&data)
            .map_err(|e| anyhow::anyhow!("parsing creature-tools.json: {e}"))?;
        Ok(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/creature-tools.json");

    #[test]
    fn load_valid_parses_all_fields() {
        let cfg = Config::load_from(FIXTURE).expect("fixture must parse");
        assert_eq!(cfg.creatures_json, "creatures.json");
        assert_eq!(cfg.moves_json, "moves.json");
        assert_eq!(cfg.map_json, "map.json");
        assert_eq!(cfg.tileset_png, "tileset.png");
        assert_eq!(cfg.map_tileset_png, "map_tileset.png");
        assert_eq!(cfg.npc_tileset_png, "npc_tileset.png");
        assert_eq!(cfg.output_dir, "out");
    }

    #[test]
    fn load_missing_errors() {
        let out = Config::load_from("/nonexistent/creature-tools.json");
        assert!(out.is_err(), "expected error for missing file");
    }

    #[test]
    fn load_malformed_errors() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/bad-creature-tools.json");
        let out = Config::load_from(path);
        assert!(out.is_err(), "expected error for malformed JSON");
    }
}