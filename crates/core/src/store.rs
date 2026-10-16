//! JSON store for the canonical game-data files (map / creatures / moves),
//! ported from creatureTools internal/store/store.go.
use crate::blocks::BlockLibrary;
use crate::config::Config;
use crate::model::{Creature, GameMap, Move};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Loads and saves the canonical game-data JSON files. Mirrors Go's
/// `store.Store`: every save uses pretty-printed JSON with 2-space indent
/// (Go `json.MarshalIndent(x, "", "  ")`).
pub struct Store<'a> {
    cfg: &'a Config,
}

impl<'a> Store<'a> {
    pub fn new(cfg: &'a Config) -> Store<'a> {
        Store { cfg }
    }

    pub fn load_creatures(&self) -> anyhow::Result<Vec<Creature>> {
        load_json(&self.cfg.creatures_json)
    }

    pub fn save_creatures(&self, creatures: &[Creature]) -> anyhow::Result<()> {
        save_json(&self.cfg.creatures_json, creatures)
    }

    pub fn load_moves(&self) -> anyhow::Result<Vec<Move>> {
        load_json(&self.cfg.moves_json)
    }

    pub fn save_moves(&self, moves: &[Move]) -> anyhow::Result<()> {
        save_json(&self.cfg.moves_json, moves)
    }

    /// Loads either canonical GameMap JSON or a Tiled interchange map. Saving
    /// always remains canonical; Tiled is an import-only project source.
    pub fn load_map(&self) -> anyhow::Result<GameMap> {
        let path = Path::new(&self.cfg.map_json);
        let data =
            std::fs::read(path).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        let value: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))?;

        let mut map = if value.get("layers").is_some() {
            let source = std::str::from_utf8(&data)
                .map_err(|e| anyhow::anyhow!("reading Tiled map {}: {e}", path.display()))?;
            crate::tiled::import(source)
                .map_err(|e| anyhow::anyhow!("importing Tiled map {}: {e}", path.display()))?
        } else {
            serde_json::from_value(value)
                .map_err(|e| anyhow::anyhow!("parsing canonical map {}: {e}", path.display()))?
        };
        resolve_tile_properties(&mut map, path)?;
        Ok(map)
    }

    pub fn save_map(&self, map: &GameMap) -> anyhow::Result<()> {
        save_json(&self.cfg.map_json, map)
    }

    /// Loads the project block library. A project that has never saved blocks
    /// has no file yet, which is an empty library rather than an error.
    pub fn load_blocks(&self) -> anyhow::Result<BlockLibrary> {
        let path = self.blocks_path()?;
        if !Path::new(&path).exists() {
            return Ok(BlockLibrary::default());
        }
        let library: BlockLibrary = load_json(&path)?;
        library.validate().map_err(|errors| {
            anyhow::anyhow!("invalid blocks file {path}: {}", errors.join("; "))
        })?;
        Ok(library)
    }

    pub fn save_blocks(&self, library: &BlockLibrary) -> anyhow::Result<()> {
        library
            .validate()
            .map_err(|errors| anyhow::anyhow!("refusing to save blocks: {}", errors.join("; ")))?;
        let path = self.blocks_path()?;
        if let Some(parent) = Path::new(&path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| anyhow::anyhow!("creating {}: {e}", parent.display()))?;
        }
        save_json(&path, library)
    }

    fn blocks_path(&self) -> anyhow::Result<String> {
        self.cfg
            .blocks_json
            .clone()
            .ok_or_else(|| anyhow::anyhow!("project config has no blocks file path"))
    }
}

const TILE_PROPERTIES: [(&str, u8); 6] = [
    ("walkable", 1 << 0),
    ("water", 1 << 1),
    ("plantable", 1 << 2),
    ("lureBox", 1 << 3),
    ("encounter", 1 << 4),
    ("spare", 1 << 5),
];

struct LoadedTileset {
    firstgid: u32,
    tilecount: u32,
    properties: BTreeMap<u32, u8>,
    path: PathBuf,
}

fn resolve_tile_properties(map: &mut GameMap, map_path: &Path) -> anyhow::Result<()> {
    map.source_path = Some(map_path.to_path_buf());
    map.tile_properties.clear();
    if map.tilesets.is_empty() {
        return Ok(());
    }

    let map_dir = map_path.parent().unwrap_or_else(|| Path::new("."));
    let mut tilesets = Vec::with_capacity(map.tilesets.len());
    for reference in &map.tilesets {
        if reference.firstgid == 0 {
            anyhow::bail!(
                "{}: tileset `{}` has firstgid 0",
                map_path.display(),
                reference.source
            );
        }
        let tsx_path = map_dir.join(&reference.source);
        let source = std::fs::read_to_string(&tsx_path).map_err(|error| {
            anyhow::anyhow!(
                "{}: cannot resolve tileset `{}`: {error}",
                map_path.display(),
                tsx_path.display()
            )
        })?;
        let (tilecount, properties) = parse_tileset_properties(&source).map_err(|error| {
            anyhow::anyhow!(
                "{}: tileset {}: {error}",
                map_path.display(),
                tsx_path.display()
            )
        })?;
        tilesets.push(LoadedTileset {
            firstgid: reference.firstgid,
            tilecount,
            properties,
            path: tsx_path,
        });
    }
    tilesets.sort_by_key(|tileset| tileset.firstgid);
    for pair in tilesets.windows(2) {
        let end = pair[0]
            .firstgid
            .checked_add(pair[0].tilecount)
            .ok_or_else(|| {
                anyhow::anyhow!("{}: tileset range overflows", pair[0].path.display())
            })?;
        if end > pair[1].firstgid {
            anyhow::bail!(
                "{}: tileset ranges overlap with {}",
                pair[0].path.display(),
                pair[1].path.display()
            );
        }
    }

    for (y, row) in map.tiles.iter().enumerate() {
        for (x, &gid) in row.iter().enumerate() {
            if gid == 0 {
                continue;
            }
            if gid > 0x03FF {
                anyhow::bail!(
                    "{}: tile ({x},{y}) has GID {gid}; maximum supported GID is 1023",
                    map_path.display()
                );
            }
            let tileset = tilesets
                .iter()
                .rev()
                .find(|tileset| gid as u32 >= tileset.firstgid)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "{}: tile ({x},{y}) GID {gid} is not covered by a tileset",
                        map_path.display()
                    )
                })?;
            let local_id = gid as u32 - tileset.firstgid;
            if local_id >= tileset.tilecount {
                anyhow::bail!(
                    "{}: tile ({x},{y}) GID {gid} has unresolved local tile {local_id} in {}",
                    map_path.display(),
                    tileset.path.display()
                );
            }
            map.tile_properties
                .insert(gid, tileset.properties.get(&local_id).copied().unwrap_or(0));
        }
    }
    Ok(())
}

fn parse_tileset_properties(source: &str) -> Result<(u32, BTreeMap<u32, u8>), String> {
    if !source.contains("</tileset>") {
        return Err("missing `</tileset>`".to_string());
    }
    let (_, _, _, root_attrs) = find_open_tag(source, "tileset", 0)?
        .ok_or_else(|| "missing `<tileset>` root".to_string())?;
    let tilecount = root_attrs
        .get("tilecount")
        .ok_or_else(|| "tileset is missing `tilecount`".to_string())?
        .parse::<u32>()
        .map_err(|_| "tileset has invalid `tilecount`".to_string())?;
    let mut tile_properties = BTreeMap::new();
    let mut cursor = 0;
    while let Some((start, end, self_closing, attrs)) = find_open_tag(source, "tile", cursor)? {
        let local_id = attrs
            .get("id")
            .ok_or_else(|| "tile record is missing `id`".to_string())?
            .parse::<u32>()
            .map_err(|_| "tile record has invalid `id`".to_string())?;
        let body_end = if self_closing {
            end + 1
        } else {
            let close = source[end + 1..]
                .find("</tile>")
                .map(|offset| end + 1 + offset)
                .ok_or_else(|| format!("tile {local_id} is missing `</tile>`"))?;
            close
        };
        let body = &source[end + 1..body_end];
        let mut mask = 0;
        let mut seen: Vec<String> = Vec::new();
        let mut property_cursor = 0;
        while let Some((_, property_end, _, property_attrs)) =
            find_open_tag(body, "property", property_cursor)?
        {
            let Some(name) = property_attrs.get("name") else {
                return Err(format!("tile {local_id} has a property without `name`"));
            };
            if let Some((_, bit)) = TILE_PROPERTIES
                .iter()
                .find(|(known, _)| *known == name.as_str())
            {
                if seen.iter().any(|prior| prior == name) {
                    return Err(format!("tile {local_id} repeats property `{name}`"));
                }
                seen.push(name.clone());
                let kind = property_attrs
                    .get("type")
                    .map(String::as_str)
                    .unwrap_or("string");
                if kind != "bool" {
                    return Err(format!(
                        "tile {local_id} property `{name}` has type `{kind}`, expected `bool`"
                    ));
                }
                match property_attrs.get("value").map(String::as_str) {
                    Some("true") => mask |= *bit,
                    Some("false") => {}
                    _ => {
                        return Err(format!(
                            "tile {local_id} property `{name}` has invalid boolean value"
                        ));
                    }
                }
            }
            property_cursor = property_end + 1;
        }
        if tile_properties.insert(local_id, mask).is_some() {
            return Err(format!("duplicate tile record {local_id}"));
        }
        cursor = if self_closing {
            end + 1
        } else {
            body_end + "</tile>".len()
        };
        let _ = start;
    }
    Ok((tilecount, tile_properties))
}

fn find_open_tag(
    source: &str,
    wanted: &str,
    mut cursor: usize,
) -> Result<Option<(usize, usize, bool, BTreeMap<String, String>)>, String> {
    while let Some(relative) = source[cursor..].find('<') {
        let start = cursor + relative;
        let Some(end_relative) = source[start..].find('>') else {
            return Err("unterminated XML tag".to_string());
        };
        let end = start + end_relative;
        let inside = source[start + 1..end].trim();
        cursor = end + 1;
        if inside.starts_with('/') || inside.starts_with('!') || inside.starts_with('?') {
            continue;
        }
        let self_closing = inside.ends_with('/');
        let tag = inside.trim_end_matches('/').trim();
        let name_end = tag.find(char::is_whitespace).unwrap_or(tag.len());
        if &tag[..name_end] != wanted {
            continue;
        }
        let attrs = parse_xml_attributes(&tag[name_end..])?;
        return Ok(Some((start, end, self_closing, attrs)));
    }
    Ok(None)
}

fn parse_xml_attributes(source: &str) -> Result<BTreeMap<String, String>, String> {
    let mut attributes = BTreeMap::new();
    let bytes = source.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() {
            break;
        }
        let name_start = cursor;
        while cursor < bytes.len() && !bytes[cursor].is_ascii_whitespace() && bytes[cursor] != b'='
        {
            cursor += 1;
        }
        if cursor == name_start {
            return Err("malformed XML attribute name".to_string());
        }
        let name = std::str::from_utf8(&bytes[name_start..cursor])
            .map_err(|_| "XML attribute name is not UTF-8".to_string())?;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() || bytes[cursor] != b'=' {
            return Err(format!("XML attribute `{name}` is missing `=`"));
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() || !matches!(bytes[cursor], b'\'' | b'"') {
            return Err(format!("XML attribute `{name}` must be quoted"));
        }
        let quote = bytes[cursor];
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        if cursor == bytes.len() {
            return Err(format!("XML attribute `{name}` has an unterminated value"));
        }
        let value = std::str::from_utf8(&bytes[value_start..cursor])
            .map_err(|_| format!("XML attribute `{name}` is not UTF-8"))?;
        if attributes
            .insert(name.to_string(), value.to_string())
            .is_some()
        {
            return Err(format!("duplicate XML attribute `{name}`"));
        }
        cursor += 1;
    }
    Ok(attributes)
}

fn load_json<T: serde::de::DeserializeOwned>(path: impl AsRef<Path>) -> anyhow::Result<T> {
    let path = path.as_ref();
    let data =
        std::fs::read(path).map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
    serde_json::from_slice(&data).map_err(|e| anyhow::anyhow!("parsing {}: {e}", path.display()))
}

fn save_json<T: serde::Serialize + ?Sized>(
    path: impl AsRef<Path>,
    value: &T,
) -> anyhow::Result<()> {
    let path = path.as_ref();
    let data = serde_json::to_string_pretty(value)
        .map_err(|e| anyhow::anyhow!("serializing {}: {e}", path.display()))?;
    std::fs::write(path, data).map_err(|e| anyhow::anyhow!("writing {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EncounterZone, NPCObject};

    const TD: &str = env!("CARGO_MANIFEST_DIR");
    const CREATURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/creatures.json");
    const MOVES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/moves.json");
    const MAP: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/map.json");
    const TILE_PROPERTY_MAP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/tile_properties_map.json"
    );

    fn cfg() -> Config {
        Config {
            creatures_json: CREATURES.to_string(),
            moves_json: MOVES.to_string(),
            map_json: MAP.to_string(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: String::new(),
            fixture_dir: None,
            battle_presets: None,
            arena_demo: None,
            duel: None,
            fx_layout: None,
            blocks_json: None,
        }
    }

    #[test]
    fn load_creatures_fixture_fields() {
        let c = cfg();
        let s = Store::new(&c);
        let creatures = s.load_creatures().expect("fixture must parse");
        assert_eq!(creatures.len(), 2);
        assert_eq!(creatures[0].name, "SquibbleSnail");
        assert_eq!(creatures[0].id, 0);
        assert_eq!(creatures[0].type1, "wind");
        assert_eq!(creatures[0].type2, "none");
        assert_eq!(creatures[0].evo_level, 8);
        assert_eq!(creatures[0].atk, 3);
        assert_eq!(creatures[0].def, 3);
        assert_eq!(creatures[0].spc_atk, 2);
        assert_eq!(creatures[0].spc_def, 2);
        assert_eq!(creatures[0].hp, 3);
        assert_eq!(creatures[0].spd, 3);
        assert_eq!(creatures[0].move1, 8);
        assert_eq!(creatures[0].move2, 32);
        assert_eq!(creatures[0].move_list.len(), 1);
        assert_eq!(creatures[0].move_list[0].name, "thought");
        assert_eq!(creatures[0].move_list[0].r#type, "spirit");
        assert_eq!(creatures[1].name, "NewOne");
        assert_eq!(creatures[1].id, 99);
        assert!(creatures[1].move_list.is_empty());
    }

    #[test]
    fn load_moves_fixture_fields() {
        let c = cfg();
        let s = Store::new(&c);
        let moves = s.load_moves().expect("fixture must parse");
        assert_eq!(moves.len(), 2);
        assert_eq!(moves[0].id, 0);
        assert_eq!(moves[0].name, "smite");
        assert_eq!(moves[0].r#type, "spirit");
        assert_eq!(moves[0].power, 10);
        assert_eq!(moves[0].physical, 0);
        assert_eq!(moves[0].accuracy, 100);
        assert_eq!(moves[0].effect, "NONE");
    }

    #[test]
    fn load_map_fixture_fields() {
        let c = cfg();
        let s = Store::new(&c);
        let m = s.load_map().expect("fixture must parse");
        assert_eq!(m.width, 3);
        assert_eq!(m.height, 2);
        assert_eq!(m.tiles.len(), 2);
        assert_eq!(m.tiles[0], vec![1, 2, 3]);
        assert_eq!(m.collision.len(), 2);
        assert_eq!(m.collision[1][0], 1);
        assert_eq!(m.collision_types.len(), 2);
        assert_eq!(m.collision_types[0].color, "#FF000080");
        assert_eq!(m.encounters[1][1], -1);
        assert_eq!(m.encounter_zones.len(), 1);
        assert_eq!(m.encounter_zones[0].creature_ids, vec![0, 5]);
        assert_eq!(m.npcs.len(), 1);
        assert_eq!(m.npcs[0].sprite_id, 12);
        assert_eq!(m.scripts.len(), 1);
        assert_eq!(m.scripts[0].script, "talk");
    }

    #[test]
    fn load_map_resolves_tsx_properties_relative_to_map_and_keeps_gids() {
        let mut c = cfg();
        c.map_json = TILE_PROPERTY_MAP.to_string();
        let map = Store::new(&c).load_map().expect("TSX properties resolve");
        assert_eq!(map.tiles[0], vec![0, 527, 1023]);
        assert_eq!(map.tile_properties.get(&527), Some(&0b000011));
        assert_eq!(map.tile_properties.get(&1023), Some(&0b110000));
        let saved = serde_json::to_value(&map).expect("map serializes");
        assert_eq!(saved["tilesets"][0]["firstgid"], 1);
        assert_eq!(saved["tilesets"][0]["source"], "tile_properties.tsx");
    }

    #[test]
    fn tsx_parser_preserves_every_six_bit_property_combination() {
        let mut source = String::from("<tileset tilecount=\"64\">\n");
        for mask in 0..64u8 {
            source.push_str(&format!("<tile id=\"{mask}\"><properties>"));
            for (name, bit) in TILE_PROPERTIES {
                if mask & bit != 0 {
                    source.push_str(&format!(
                        "<property name=\"{name}\" type=\"bool\" value=\"true\"/>"
                    ));
                }
            }
            source.push_str("</properties></tile>\n");
        }
        source.push_str("</tileset>");
        let (tilecount, properties) = parse_tileset_properties(&source).expect("all masks parse");
        assert_eq!(tilecount, 64);
        for mask in 0..64u32 {
            assert_eq!(properties.get(&mask), Some(&(mask as u8)));
        }
    }

    #[test]
    fn tsx_parser_rejects_non_boolean_known_properties() {
        let source = r#"<tileset tilecount="1"><tile id="0"><properties><property name="walkable" type="int" value="1"/></properties></tile></tileset>"#;
        let error = parse_tileset_properties(source).expect_err("invalid type must fail");
        assert!(
            error.contains("tile 0 property `walkable` has type `int`"),
            "{error}"
        );
    }

    /// Saves land in dedicated roundtrip_* files so the read-only fixtures
    /// stay pristine (mirrors Go's setup which copied fixtures into a fresh
    /// working dir).
    fn roundtrip_cfg(what: &str) -> Config {
        let base = cfg();
        let mut c = base.clone();
        match what {
            "creatures" => c.creatures_json = format!("{TD}/testdata/roundtrip_creatures.json"),
            "moves" => c.moves_json = format!("{TD}/testdata/roundtrip_moves.json"),
            "map" => c.map_json = format!("{TD}/testdata/roundtrip_map.json"),
            _ => unreachable!(),
        }
        c
    }

    /// Copy the fixture into the roundtrip path so the load half of the
    /// round-trip reads the same data Go's setup produced.
    fn seed_roundtrip(from: &str, to: &str) {
        std::fs::copy(from, to).expect("seeding roundtrip fixture");
    }

    #[test]
    fn roundtrip_creatures() {
        seed_roundtrip(CREATURES, &roundtrip_cfg("creatures").creatures_json);
        let c = roundtrip_cfg("creatures");
        let s = Store::new(&c);
        let orig = s.load_creatures().unwrap();
        s.save_creatures(&orig).unwrap();
        let reloaded = s.load_creatures().unwrap();
        let a = serde_json::to_string_pretty(&orig).unwrap();
        let b = serde_json::to_string_pretty(&reloaded).unwrap();
        assert_eq!(a, b, "creatures round-trip mismatch");
        assert_eq!(orig, reloaded);
    }

    #[test]
    fn roundtrip_moves() {
        seed_roundtrip(MOVES, &roundtrip_cfg("moves").moves_json);
        let c = roundtrip_cfg("moves");
        let s = Store::new(&c);
        let orig = s.load_moves().unwrap();
        s.save_moves(&orig).unwrap();
        let reloaded = s.load_moves().unwrap();
        assert_eq!(orig, reloaded, "moves round-trip mismatch");
    }

    #[test]
    fn roundtrip_map() {
        seed_roundtrip(MAP, &roundtrip_cfg("map").map_json);
        let c = roundtrip_cfg("map");
        let s = Store::new(&c);
        let orig = s.load_map().unwrap();
        s.save_map(&orig).unwrap();
        let reloaded = s.load_map().unwrap();
        assert_eq!(orig, reloaded, "map round-trip mismatch");
    }

    #[test]
    fn load_missing_creatures_errors() {
        let c = Config {
            creatures_json: "/nonexistent/path.json".to_string(),
            moves_json: "/nonexistent/moves.json".to_string(),
            map_json: MAP.to_string(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: String::new(),
            fixture_dir: None,
            battle_presets: None,
            arena_demo: None,
            duel: None,
            fx_layout: None,
            blocks_json: None,
        };
        let s = Store::new(&c);
        assert!(s.load_creatures().is_err(), "expected error");
        assert!(s.load_moves().is_err(), "expected error");
    }

    /// Go parity: `json.Unmarshal` tolerates a map JSON that omits the
    /// collision/encounter grids (e.g. the hand-authored tools_dev_map.json);
    /// serde must do the same instead of erroring on missing fields.
    #[test]
    fn load_partial_map_defaults_missing_grids() {
        let partial = format!("{TD}/testdata/roundtrip_partial_map.json");
        std::fs::write(
            &partial,
            r#"{"width":16,"height":16,"tiles":[],"npcs":[],"scripts":[]}"#,
        )
        .expect("write partial map");
        let c = Config {
            creatures_json: CREATURES.to_string(),
            moves_json: MOVES.to_string(),
            map_json: partial,
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: String::new(),
            fixture_dir: None,
            battle_presets: None,
            arena_demo: None,
            duel: None,
            fx_layout: None,
            blocks_json: None,
        };
        let s = Store::new(&c);
        let m = s.load_map().expect("partial map must load like Go");
        assert_eq!(m.width, 16);
        assert_eq!(m.height, 16);
        assert!(m.collision.is_empty());
        assert!(m.collision_types.is_empty());
        assert!(m.encounters.is_empty());
        assert!(m.encounter_zones.is_empty());
    }

    #[test]
    fn load_missing_map_errors() {
        let c = Config {
            creatures_json: CREATURES.to_string(),
            moves_json: MOVES.to_string(),
            map_json: "/nonexistent/map.json".to_string(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: String::new(),
            fixture_dir: None,
            battle_presets: None,
            arena_demo: None,
            duel: None,
            fx_layout: None,
            blocks_json: None,
        };
        let s = Store::new(&c);
        assert!(s.load_map().is_err(), "expected error");
    }

    #[test]
    fn save_pretty_indent_two() {
        // Mirrors Go's MarshalIndent(x, "", "  "): 2-space indent.
        // Distinct file from roundtrip_map so parallel tests don't race.
        let mut c = roundtrip_cfg("map");
        c.map_json = format!("{TD}/testdata/roundtrip_map_pretty.json");
        seed_roundtrip(MAP, &c.map_json);
        let s = Store::new(&c);
        let m = s.load_map().unwrap();
        s.save_map(&m).unwrap();
        let text = std::fs::read_to_string(&c.map_json).unwrap();
        assert!(text.contains("\n  \"width\":"), "expected 2-space indent");
    }

    fn blocks_cfg(name: &str) -> (Config, std::path::PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "cgfx-blocks-{name}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).expect("temp blocks dir");
        let mut c = cfg();
        c.blocks_json = Some(directory.join("blocks.json").to_string_lossy().into_owned());
        (c, directory)
    }

    #[test]
    fn load_blocks_without_a_file_is_an_empty_library() {
        let (c, directory) = blocks_cfg("missing");
        let library = Store::new(&c)
            .load_blocks()
            .expect("absent blocks file must load as empty");
        assert_eq!(library, crate::blocks::BlockLibrary::default());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn roundtrip_blocks() {
        let (c, directory) = blocks_cfg("roundtrip");
        let store = Store::new(&c);
        let library = crate::blocks::BlockLibrary::new(vec![crate::blocks::Block {
            id: 1,
            name: "hut".to_string(),
            w: 2,
            h: 2,
            tiles: "4,5,6,7".to_string(),
            tags: vec!["village".to_string()],
        }]);
        store.save_blocks(&library).expect("blocks must save");
        assert_eq!(store.load_blocks().expect("blocks must reload"), library);
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn save_blocks_rejects_tile_payload_mismatch() {
        let (c, directory) = blocks_cfg("invalid");
        let library = crate::blocks::BlockLibrary::new(vec![crate::blocks::Block {
            id: 1,
            name: "bad".to_string(),
            w: 2,
            h: 2,
            tiles: "4,5".to_string(),
            tags: vec![],
        }]);
        let err = Store::new(&c)
            .save_blocks(&library)
            .expect_err("mismatched tiles must not be written");
        assert!(err.to_string().contains("refusing to save blocks"));
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn save_blocks_creates_missing_parent_directories() {
        let (mut c, directory) = blocks_cfg("nested");
        c.blocks_json = Some(
            directory
                .join("data/json/blocks.json")
                .to_string_lossy()
                .into_owned(),
        );
        let store = Store::new(&c);
        store
            .save_blocks(&crate::blocks::BlockLibrary::new(vec![]))
            .expect("nested blocks path must be created");
        assert!(directory.join("data/json/blocks.json").is_file());
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn json_keys_match_go_tags() {
        // Assert that the serialized JSON keys are exactly the Go struct tags,
        // including the rename cases (evelevel/deff/spcatk/spcdef/moveList/...).
        let c = Creature {
            name: "N".to_string(),
            id: 1,
            type1: "wind".to_string(),
            type2: "none".to_string(),
            evo_level: 5,
            atk: 1,
            def: 2,
            spc_atk: 3,
            spc_def: 4,
            hp: 5,
            spd: 6,
            move1: 0,
            move2: 0,
            move3: 0,
            move4: 0,
            move_list: vec![],
        };
        let v: serde_json::Value = serde_json::to_value(&c).unwrap();
        let o = v.as_object().unwrap();
        for key in [
            "name", "id", "type1", "type2", "evelevel", "atk", "deff", "spcatk", "spcdef", "hp",
            "spd", "move1", "move2", "move3", "move4", "moveList",
        ] {
            assert!(o.contains_key(key), "missing key {key}");
        }
        assert!(!o.contains_key("evo_level"));

        let z = EncounterZone {
            id: 1,
            name: "z".to_string(),
            rate: 50,
            creature_ids: vec![1, 2],
        };
        let v: serde_json::Value = serde_json::to_value(&z).unwrap();
        assert!(v.as_object().unwrap().contains_key("creature_ids"));

        let n = NPCObject {
            id: 1,
            x: 0,
            y: 0,
            sprite_id: 9,
            script: "s".to_string(),
        };
        let v: serde_json::Value = serde_json::to_value(&n).unwrap();
        assert!(v.as_object().unwrap().contains_key("sprite_id"));
    }
}
