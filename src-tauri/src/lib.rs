//! Tauri 2 command layer: IPC wrappers over `creaturevm` (cgfx-core).
//!
//! Every command returns a structured, serializable value or
//! `Result<T, String>` — no panic across IPC, no clap in this layer.

use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use creaturevm::builder;
use creaturevm::config::Config;
use creaturevm::fxpack::{
    layout::{Field, FieldKind, FieldSource, FLASH_PAGES, PAGE_SIZE},
    pack::{self, BuilderBytes, PackPlan},
    spec::LayoutSpec,
};
use creaturevm::model::{Creature, GameMap, Move};
use creaturevm::processor::ast::{Branch, Cmd, Condition, Location};
use creaturevm::script::{self, Controller};
use creaturevm::store::Store;
use creaturevm::wfcmap::{
    capture_piece as capture_wfc_piece, generate as generate_wfc, CaptureOpts, Hunk, TileRect,
    ValidationIssue, WfcPiece, WfcProject,
};
use serde::Serialize;
use tauri::{Manager, State};

/// Runtime state managed by the app. Store borrows Config, so stores are
/// constructed per operation from the locked config.
struct AppState {
    config: Config,
}

fn err_str<E: Display>(e: E) -> String {
    e.to_string()
}

// ─────────────────────────────────────────────────────────────────────
// Result types (serialized to the frontend)
// ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct CompileError {
    pub line: usize,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompileResult {
    pub ok: bool,
    pub errors: Vec<CompileError>,
    /// Written artifact file paths (absolute-ish: `output_dir/name`).
    pub artifacts: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScriptErr {
    pub line: usize,
    pub col: usize,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CmdJson {
    pub opcode: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParseResult {
    pub ok: bool,
    pub errors: Vec<ScriptErr>,
    pub cmds: Option<Vec<CmdJson>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutImage {
    pub frames: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutField {
    pub name: String,
    pub namespace: Option<String>,
    pub addr: u32,
    pub len: u32,
    pub span: u32,
    pub kind: String,
    pub in_save: bool,
    pub source: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<LayoutImage>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LayoutReport {
    pub data_page: u16,
    pub save_page: u16,
    pub data_len: u32,
    pub data_len_flash: u32,
    pub save_len: u32,
    pub save_len_flash: u32,
    pub page_size: u32,
    pub flash_pages: u32,
    pub fields: Vec<LayoutField>,
}

fn builder_bin_name(key: &str) -> &str {
    match key {
        "creature_data" => "creatureData.bin",
        "move_table" => "moves.bin",
        "opponents" => "opponents.bin",
        "type_table" => "typetable.bin",
        "map_data" => "map.bin",
        "raw_map" => "raw_map.bin",
        "text" => "text.bin",
        "scripts" => "scripts.bin",
        "tool_version" => "tool_version.bin",
        other => other,
    }
}

fn load_layout_builders<'a>(
    spec: &'a LayoutSpec,
    output_dir: &Path,
) -> Result<(Vec<BuilderBytes<'a>>, HashMap<String, PathBuf>), String> {
    let mut builders = Vec::new();
    let mut paths = HashMap::new();
    for key in spec.builder_keys() {
        let path = output_dir.join(builder_bin_name(key));
        if !path.is_file() {
            return Err(format!(
                "missing builder {key:?} at {}; run Compile all",
                path.display()
            ));
        }
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("reading builder {key:?} from {}: {e}", path.display()))?;
        paths.insert(key.to_owned(), path);
        builders.push(BuilderBytes {
            key,
            bytes,
            namespace: None,
        });
    }
    Ok((builders, paths))
}

fn layout_field(field: Field, builder_paths: &HashMap<String, PathBuf>) -> LayoutField {
    let (kind, image) = match field.kind {
        FieldKind::Bytes => ("bytes".to_owned(), None),
        FieldKind::Image {
            frames,
            width,
            height,
        } => (
            "image".to_owned(),
            Some(LayoutImage {
                frames,
                width,
                height,
            }),
        ),
    };
    let (source, path) = match field.source {
        FieldSource::Raw { path } => ("raw", path),
        FieldSource::CArray { path } => ("carray", path),
        FieldSource::Symbol { path } => ("symbol", path),
        FieldSource::Image { path } => ("image", path),
        FieldSource::Builder { key } => (
            "builder",
            builder_paths
                .get(&key)
                .cloned()
                .unwrap_or_else(|| PathBuf::from(key)),
        ),
        FieldSource::Save | FieldSource::Unknown => ("raw", PathBuf::new()),
    };
    LayoutField {
        name: field.name,
        namespace: field.namespace,
        addr: field.addr,
        len: field.source_len,
        span: field.span,
        kind,
        in_save: field.in_save,
        source: source.to_owned(),
        path: path.to_string_lossy().into_owned(),
        image,
    }
}

fn layout_report_from_plan(
    plan: PackPlan,
    builder_paths: &HashMap<String, PathBuf>,
) -> LayoutReport {
    LayoutReport {
        data_page: plan.offsets.data_page,
        save_page: plan.offsets.save_page.unwrap_or(0),
        data_len: plan.offsets.data_len,
        data_len_flash: plan.offsets.data_len_flash,
        save_len: plan.offsets.save_len,
        save_len_flash: plan.offsets.save_len_flash,
        page_size: PAGE_SIZE,
        flash_pages: FLASH_PAGES,
        fields: plan
            .fields
            .into_iter()
            .map(|field| layout_field(field, builder_paths))
            .collect(),
    }
}

const LAYOUT_SPEC_REMEDY: &str =
    "set fx_layout in creature-tools.json or check the FX checkout path";

fn layout_spec_path(cfg: &Config) -> Result<PathBuf, String> {
    if let Some(path) = &cfg.fx_layout {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "layout spec unavailable at {}; {LAYOUT_SPEC_REMEDY}",
            path.display()
        ));
    }

    let output_dir = Path::new(&cfg.output_dir);
    for project_root in output_dir.ancestors() {
        let layout_path = project_root.join("fxlayout.toml");
        if layout_path.is_file() {
            return Ok(layout_path);
        }
    }
    Err(format!(
        "layout spec unavailable: no fxlayout.toml found walking up from {}; {LAYOUT_SPEC_REMEDY}",
        output_dir.display()
    ))
}

fn layout_report_impl(cfg: &Config) -> Result<LayoutReport, String> {
    let layout_path = layout_spec_path(cfg)?;
    let spec = LayoutSpec::load(&layout_path).map_err(|e| {
        format!(
            "loading layout spec {}: {e}; {LAYOUT_SPEC_REMEDY}",
            layout_path.display()
        )
    })?;
    let (builders, builder_paths) = load_layout_builders(&spec, Path::new(&cfg.output_dir))?;
    let plan = pack::plan(&spec, &builders).map_err(err_str)?;
    Ok(layout_report_from_plan(plan, &builder_paths))
}

#[tauri::command]
fn layout_report(state: State<Mutex<AppState>>) -> Result<LayoutReport, String> {
    let st = state.lock().map_err(err_str)?;
    layout_report_impl(&st.config)
}

// ─────────────────────────────────────────────────────────────────────
// Config
// ─────────────────────────────────────────────────────────────────────

#[tauri::command]
fn load_config(state: State<Mutex<AppState>>) -> Result<Config, String> {
    let st = state.lock().map_err(err_str)?;
    Ok(st.config.clone())
}

#[tauri::command]
fn save_config(state: State<Mutex<AppState>>, config: Config) -> Result<(), String> {
    // Persist to creature-tools.json (same path Config::load reads) so the
    // saved config survives restarts.
    let json = serde_json::to_string_pretty(&config).map_err(err_str)?;
    std::fs::write("creature-tools.json", json).map_err(err_str)?;
    let mut st = state.lock().map_err(err_str)?;
    st.config = config;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────
// Store (canonical game data: map / creatures / moves)
// ─────────────────────────────────────────────────────────────────────

/// Run a store operation against the state's loaded config. The lock is
/// held for the duration of the operation (short-lived I/O only).
fn with_store<T>(
    state: &State<Mutex<AppState>>,
    f: impl FnOnce(&Store) -> anyhow::Result<T>,
) -> Result<T, String> {
    let st = state.lock().map_err(err_str)?;
    let store = Store::new(&st.config);
    f(&store).map_err(err_str)
}

#[tauri::command]
fn load_map(state: State<Mutex<AppState>>) -> Result<GameMap, String> {
    with_store(&state, |s| s.load_map())
}

#[tauri::command]
fn save_map(state: State<Mutex<AppState>>, map: GameMap) -> Result<(), String> {
    with_store(&state, |s| s.save_map(&map))
}

#[tauri::command]
fn load_creatures(state: State<Mutex<AppState>>) -> Result<Vec<Creature>, String> {
    with_store(&state, |s| s.load_creatures())
}

#[tauri::command]
fn save_creatures(state: State<Mutex<AppState>>, creatures: Vec<Creature>) -> Result<(), String> {
    with_store(&state, |s| s.save_creatures(&creatures))
}

#[tauri::command]
fn load_moves(state: State<Mutex<AppState>>) -> Result<Vec<Move>, String> {
    with_store(&state, |s| s.load_moves())
}

#[tauri::command]
fn save_moves(state: State<Mutex<AppState>>, moves: Vec<Move>) -> Result<(), String> {
    with_store(&state, |s| s.save_moves(&moves))
}

// ─────────────────────────────────────────────────────────────────────
// Compile (map / creatures / moves builders)
// ─────────────────────────────────────────────────────────────────────

const MAP_ARTIFACTS: [&str; 9] = [
    "raw_map.bin",
    "scripts.bin",
    "text.bin",
    "opcodes.hpp",
    "flags.hpp",
    "flag_bit_array.hpp",
    "flag_bit_array.cpp",
    "locations.hpp",
    "scripts.hpp",
];

const CREATURES_ARTIFACTS: [&str; 2] = ["creatureData.bin", "creatureData.hpp"];

const MOVES_ARTIFACTS: [&str; 2] = ["moves.bin", "moves.hpp"];

fn artifact_paths(cfg: &Config, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .map(|n| format!("{}/{}", cfg.output_dir, n))
        .collect()
}

fn build_error(e: anyhow::Error) -> CompileResult {
    CompileResult {
        ok: false,
        errors: vec![CompileError {
            line: 0,
            msg: e.to_string(),
        }],
        artifacts: vec![],
    }
}

fn builder_error(e: builder::BuildError) -> CompileResult {
    CompileResult {
        ok: false,
        errors: vec![CompileError { line: 0, msg: e.0 }],
        artifacts: vec![],
    }
}

fn compile_map_impl(cfg: &Config) -> CompileResult {
    // Load the canonical map from the store (paths from config).
    let map = match Store::new(cfg).load_map() {
        Ok(m) => m,
        Err(e) => return build_error(e),
    };
    match builder::write_map(cfg, &map) {
        Ok(()) => CompileResult {
            ok: true,
            errors: vec![],
            artifacts: artifact_paths(cfg, &MAP_ARTIFACTS),
        },
        Err(e) => builder_error(e),
    }
}

fn compile_creatures_impl(cfg: &Config) -> CompileResult {
    let creatures = match Store::new(cfg).load_creatures() {
        Ok(c) => c,
        Err(e) => return build_error(e),
    };
    match builder::write_creatures(cfg, &creatures) {
        Ok(()) => CompileResult {
            ok: true,
            errors: vec![],
            artifacts: artifact_paths(cfg, &CREATURES_ARTIFACTS),
        },
        Err(e) => builder_error(e),
    }
}

fn compile_moves_impl(cfg: &Config) -> CompileResult {
    let moves = match Store::new(cfg).load_moves() {
        Ok(m) => m,
        Err(e) => return build_error(e),
    };
    match builder::write_moves(cfg, &moves) {
        Ok(()) => CompileResult {
            ok: true,
            errors: vec![],
            artifacts: artifact_paths(cfg, &MOVES_ARTIFACTS),
        },
        Err(e) => builder_error(e),
    }
}

/// Compile just the map. Reads the map from the store; errors carry the
/// message text (never panic).
#[tauri::command]
fn compile_map(state: State<Mutex<AppState>>) -> CompileResult {
    let st = match state.lock() {
        Ok(s) => s,
        Err(_) => {
            return CompileResult {
                ok: false,
                errors: vec![CompileError {
                    line: 0,
                    msg: "state lock poisoned".into(),
                }],
                artifacts: vec![],
            };
        }
    };
    compile_map_impl(&st.config)
}

/// Compile map + creatures + moves. Attempts each independently and
/// collects every error; `artifacts` lists everything written on success.
#[tauri::command]
fn compile_all(state: State<Mutex<AppState>>) -> CompileResult {
    let st = match state.lock() {
        Ok(s) => s,
        Err(_) => {
            return CompileResult {
                ok: false,
                errors: vec![CompileError {
                    line: 0,
                    msg: "state lock poisoned".into(),
                }],
                artifacts: vec![],
            };
        }
    };
    let cfg = &st.config;

    let mut result = CompileResult {
        ok: true,
        errors: vec![],
        artifacts: vec![],
    };

    for part in [
        compile_map_impl(cfg),
        compile_creatures_impl(cfg),
        compile_moves_impl(cfg),
    ] {
        result.ok &= part.ok;
        result.errors.extend(part.errors);
        result.artifacts.extend(part.artifacts);
    }
    result
}

// ─────────────────────────────────────────────────────────────────────
// Script parsing (live editor feedback)
// ─────────────────────────────────────────────────────────────────────

fn opcode_name(cmd: &Cmd) -> &'static str {
    match cmd {
        Cmd::End => "End",
        _ => Cmd::VARIANT_NAMES[cmd.opcode() as usize],
    }
}

fn loc_arg(loc: &Location) -> String {
    match loc {
        Location::Cords(x, y) => format!("({x},{y})"),
        Location::Tag(t) => format!("@{},{}", t.text, t.index),
    }
}

fn cond_arg(cond: &Condition) -> String {
    match cond {
        Condition::FlagSet(t) => format!("flag:{}", t.text),
        Condition::FlagClear(t) => format!("!flag:{}", t.text),
    }
}

/// Flatten a Cmd into (opcode name, display args) for the frontend.
fn cmd_json(cmd: &Cmd) -> CmdJson {
    let mut args: Vec<String> = Vec::new();
    match cmd {
        Cmd::Msg { text } => args.push(text.text.clone()),
        Cmd::TMsg { at, text } => {
            args.push(loc_arg(at));
            args.push(text.text.clone());
        }
        Cmd::SMsg { selected, text } => {
            args.push(loc_arg(selected));
            args.push(text.text.clone());
        }
        Cmd::Tp { to } => args.push(loc_arg(to)),
        Cmd::TpIf { from, to } => {
            args.push(loc_arg(from));
            args.push(loc_arg(to));
        }
        Cmd::If {
            condition,
            branches,
        } => {
            args.push(cond_arg(condition));
            match branches {
                Branch::Then(_) => args.push("then".into()),
                Branch::ThenElse(_, _) => args.push("then-else".into()),
            }
        }
        Cmd::SetFlag { flag } | Cmd::UnsetFlag { flag } | Cmd::ReadFlag { flag } => {
            args.push(flag.text.clone());
        }
        Cmd::End => {}
    }
    CmdJson {
        opcode: opcode_name(cmd).to_string(),
        args,
    }
}

fn parse_script_impl(src: &str) -> ParseResult {
    let mut ctrl = Controller::new();
    let locations: HashMap<String, (u16, u16)> = HashMap::new();
    match script::parse(src, &locations, &mut ctrl) {
        Ok(cmds) => ParseResult {
            ok: true,
            errors: vec![],
            cmds: Some(cmds.iter().map(cmd_json).collect()),
        },
        Err(e) => ParseResult {
            ok: false,
            errors: vec![ScriptErr {
                line: e.line,
                col: e.col,
                msg: e.msg,
            }],
            cmds: None,
        },
    }
}

/// Parse a script source string; never panics. Used by the live editor.
#[tauri::command]
fn parse_script(src: String) -> ParseResult {
    parse_script_impl(&src)
}

#[tauri::command]
fn capture_piece(map: GameMap, rect: TileRect, opts: CaptureOpts) -> Result<WfcPiece, String> {
    let project = map
        .wfc
        .as_ref()
        .ok_or_else(|| "map has no WFC project; create one before capturing a piece".to_string())?;
    let next_id = project
        .pieces
        .iter()
        .map(|piece| piece.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "cannot allocate WFC piece id: id space exhausted".to_string())?;
    capture_wfc_piece(
        &map.tiles,
        rect,
        project.slot_w,
        project.slot_h,
        next_id,
        &opts,
    )
    .map_err(err_str)
}

#[tauri::command]
fn wfc_generate(
    project: WfcProject,
    recipe_id: u32,
    size: Option<(u32, u32)>,
    seed: Option<u64>,
) -> Result<Hunk, String> {
    generate_wfc(&project, recipe_id, size, seed)
        .map_err(|error| format!("WFC generation failed: {error}"))
}

#[tauri::command]
fn wfc_validate(project: WfcProject) -> Result<Vec<ValidationIssue>, String> {
    Ok(project.validate().err().unwrap_or_default())
}

// ─────────────────────────────────────────────────────────────────────
// Tiled JSON interchange
// ─────────────────────────────────────────────────────────────────────

#[tauri::command]
fn tiled_import_path(path: String, current: GameMap) -> Result<GameMap, String> {
    let data = std::fs::read_to_string(&path).map_err(err_str)?;
    let mut imported = creaturevm::tiled::import(&data).map_err(|e| e.0)?;
    // Tiled has no WFC or stamp representation. Preserve the canonical
    // libraries from the map currently open instead of silently deleting them.
    imported.wfc = current.wfc;
    imported.stamps = current.stamps;
    Ok(imported)
}

#[tauri::command]
fn tiled_export_path(map: GameMap, path: String) -> Result<(), String> {
    let json = creaturevm::tiled::export(&map);
    std::fs::write(&path, json).map_err(err_str)
}

// ─────────────────────────────────────────────────────────────────────
// App entry point
// ─────────────────────────────────────────────────────────────────────

/// Fallback config for a machine with no `creature-tools.json` yet; all
/// paths empty so the UI can drive config save first.
fn default_config() -> Config {
    Config {
        creatures_json: String::new(),
        moves_json: String::new(),
        map_json: String::new(),
        tileset_png: String::new(),
        map_tileset_png: String::new(),
        npc_tileset_png: String::new(),
        output_dir: String::new(),
        fixture_dir: None,
        fx_layout: None,
    }
}

#[tauri::command]
fn ping() -> String {
    "pong".into()
}

/// Read a file's raw bytes (used by the frontend to load tileset PNGs from
/// the config paths). Never panics; errors are returned as strings.
fn read_file_bytes_impl(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(err_str)
}

#[tauri::command]
fn read_file_bytes(path: String) -> Result<Vec<u8>, String> {
    read_file_bytes_impl(&path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = Config::load().unwrap_or_else(|_| default_config());
            app.manage(Mutex::new(AppState { config }));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            ping,
            load_config,
            save_config,
            load_map,
            save_map,
            load_creatures,
            save_creatures,
            load_moves,
            save_moves,
            compile_map,
            compile_all,
            layout_report,
            parse_script,
            read_file_bytes,
            capture_piece,
            wfc_generate,
            wfc_validate,
            tiled_import_path,
            tiled_export_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ─────────────────────────────────────────────────────────────────────
// Tests (native Rust, cargo test)
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use creaturevm::processor::ast::{Text, ToBytecode};
    use creaturevm::wfcmap::{EdgeSocket, Side, SocketKind, SocketLabel, Stamp, WfcRecipe};

    #[test]
    fn parse_script_valid_reports_cmd_json() {
        let r = parse_script_impl("msg {Hi}; setflag flag_x");
        assert!(r.ok, "valid script must parse");
        let cmds = r.cmds.expect("cmds present on success");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].opcode, "Msg");
        assert_eq!(cmds[0].args, vec!["Hi"]);
        assert_eq!(cmds[1].opcode, "SetFlag");
        assert_eq!(cmds[1].args, vec!["flag_x"]);
        assert!(r.errors.is_empty());
    }

    #[test]
    fn parse_script_invalid_reports_line_col() {
        let r = parse_script_impl("msg {Hi}\nunknown_cmd");
        assert!(!r.ok, "invalid script must fail");
        assert!(r.cmds.is_none());
        assert_eq!(r.errors.len(), 1);
        assert_eq!(r.errors[0].line, 2);
        assert!(r.errors[0].msg.contains("unknown command"));
    }

    #[test]
    fn cmd_json_end_opcode() {
        let cmd = Cmd::End;
        assert_eq!(opcode_name(&cmd), "End");
        assert_eq!(cmd_json(&cmd).args.len(), 0);
    }

    #[test]
    fn cmd_json_cords_and_if() {
        let msg = Cmd::Msg {
            text: Text {
                text: "hi".into(),
                index: 0,
            },
        };
        assert_eq!(cmd_json(&msg).args, vec!["hi"]);

        let tp = Cmd::Tp {
            to: Location::Cords(1, 2),
        };
        assert_eq!(cmd_json(&tp).args, vec!["(1,2)"]);

        let ifc = Cmd::If {
            condition: Condition::FlagSet(Text {
                text: "flag_a".into(),
                index: 0,
            }),
            branches: Branch::Then(Box::new(msg.clone())),
        };
        let j = cmd_json(&ifc);
        assert_eq!(j.opcode, "If");
        assert_eq!(j.args[0], "flag:flag_a");
        assert_eq!(j.args[1], "then");
    }

    #[test]
    fn compile_result_serializes_fields() {
        let r = CompileResult {
            ok: false,
            errors: vec![CompileError {
                line: 2,
                msg: "boom".into(),
            }],
            artifacts: vec!["out/x.bin".into()],
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["errors"][0]["line"], 2);
        assert_eq!(v["errors"][0]["msg"], "boom");
        assert_eq!(v["artifacts"][0], "out/x.bin");
    }

    #[test]
    fn parse_result_serializes_fields() {
        let r = ParseResult {
            ok: false,
            errors: vec![ScriptErr {
                line: 1,
                col: 3,
                msg: "bad".into(),
            }],
            cmds: None,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["errors"][0]["col"], 3);
        assert!(v["cmds"].is_null());
    }

    #[test]
    fn artifact_paths_join_output_dir() {
        let cfg = Config {
            creatures_json: String::new(),
            moves_json: String::new(),
            map_json: String::new(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: "out".into(),
            fixture_dir: None,
            fx_layout: None,
        };
        let paths = artifact_paths(&cfg, &MAP_ARTIFACTS);
        assert_eq!(paths.len(), 9);
        assert!(paths.iter().all(|p| p.starts_with("out/")));
        assert!(paths.contains(&"out/raw_map.bin".to_string()));
    }

    #[test]
    fn compile_map_missing_input_reports_error_not_panic() {
        let cfg = Config {
            creatures_json: String::new(),
            moves_json: String::new(),
            map_json: "/nonexistent/map.json".into(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: "out".into(),
            fixture_dir: None,
            fx_layout: None,
        };
        let r = compile_map_impl(&cfg);
        assert!(!r.ok);
        assert_eq!(r.errors.len(), 1);
        assert!(r.errors[0].msg.contains("map.json"));
    }

    #[test]
    fn cmd_to_bytes_smoke() {
        // Sanity: the Cmd -> bytecode path still matches core expectations
        // (opcode 0 for Msg, u16 text index).
        let cmd = Cmd::Msg {
            text: Text {
                text: "hi".into(),
                index: 3,
            },
        };
        assert_eq!(cmd.to_bytes(), vec![0, 0, 3]);
    }

    #[test]
    fn layout_report_serializes_success_shape() {
        let report = layout_report_from_plan(
            PackPlan {
                fields: vec![Field {
                    name: "portrait".into(),
                    namespace: Some("Ui".into()),
                    addr: 12,
                    source_len: 8,
                    span: 16,
                    kind: FieldKind::Image {
                        frames: 2,
                        width: 16,
                        height: 32,
                    },
                    in_save: false,
                    source: FieldSource::Image {
                        path: PathBuf::from("sprites/portrait.png"),
                    },
                }],
                offsets: creaturevm::fxpack::layout::Offsets {
                    data_page: 42,
                    save_page: Some(65_000),
                    data_len: 20,
                    data_len_flash: 256,
                    save_len: 4_096,
                    save_len_flash: 4_096,
                },
                bytes: vec![],
            },
            &HashMap::new(),
        );
        let value = serde_json::to_value(report).expect("report must serialize");
        assert_eq!(value["data_page"], 42);
        assert_eq!(value["save_page"], 65_000);
        assert_eq!(value["page_size"], PAGE_SIZE);
        assert_eq!(value["flash_pages"], FLASH_PAGES);
        assert_eq!(value["fields"][0]["len"], 8);
        assert_eq!(value["fields"][0]["span"], 16);
        assert_eq!(value["fields"][0]["kind"], "image");
        assert_eq!(value["fields"][0]["source"], "image");
        assert_eq!(value["fields"][0]["path"], "sprites/portrait.png");
        assert_eq!(value["fields"][0]["image"]["frames"], 2);
    }

    fn layout_test_config(output_dir: PathBuf, fx_layout: Option<PathBuf>) -> Config {
        Config {
            creatures_json: String::new(),
            moves_json: String::new(),
            map_json: String::new(),
            tileset_png: String::new(),
            map_tileset_png: String::new(),
            npc_tileset_png: String::new(),
            output_dir: output_dir.to_string_lossy().into_owned(),
            fixture_dir: None,
            fx_layout: fx_layout.map(|path| path.to_string_lossy().into_owned()),
        }
    }

    #[test]
    fn layout_spec_path_uses_explicit_fx_layout() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/core/testdata/layout_resolution");
        let expected = root.join("fxlayout.toml");
        let cfg = layout_test_config(PathBuf::from("unused-output-dir"), Some(expected.clone()));
        assert_eq!(
            layout_spec_path(&cfg).expect("explicit layout path"),
            expected
        );
    }

    #[test]
    fn layout_spec_path_walks_up_from_output_dir() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/core/testdata/layout_resolution");
        let expected = root.join("fxlayout.toml");
        let cfg = layout_test_config(root.join("fxdata/generated"), None);
        assert_eq!(
            layout_spec_path(&cfg).expect("derived layout path"),
            expected
        );
    }

    #[test]
    fn layout_report_missing_spec_has_layout_remedy_not_compile_all() {
        let cfg = layout_test_config(PathBuf::from("/nonexistent/layout-report-output"), None);
        let error = layout_report_impl(&cfg).expect_err("layout spec must be missing");
        assert!(error.contains(LAYOUT_SPEC_REMEDY));
        assert!(!error.contains("Compile all"));
    }

    #[test]
    fn layout_report_missing_builder_names_path_and_compile_all() {
        use creaturevm::fxpack::spec::{Entry, LayoutSpec, Source};

        let spec = LayoutSpec {
            entries: vec![Entry {
                name: Some("map".into()),
                namespace: None,
                source: Source::Builder("map_data".into()),
                align: None,
            }],
            save: None,
        };
        let output_dir = Path::new("/nonexistent/layout-report-output");
        let error = match load_layout_builders(&spec, output_dir) {
            Err(error) => error,
            Ok(_) => panic!("builder must be missing"),
        };
        assert!(error.contains("missing builder \"map_data\""));
        assert!(error.contains("/nonexistent/layout-report-output/map.bin"));
        assert!(error.contains("run Compile all"));
        assert!(!error.contains(LAYOUT_SPEC_REMEDY));
    }

    #[test]
    fn read_file_bytes_returns_file_contents() {
        // Testdata fixture lives at workspace root (tests run from src-tauri/).
        let path = "../crates/core/testdata/map.json";
        let bytes = read_file_bytes_impl(path).expect("fixture readable");
        let expected = std::fs::read(path).expect("fixture readable");
        assert_eq!(bytes, expected);
        assert!(bytes.len() > 0);
        // Every byte is the raw value (no JSON-wrapping).
        assert_eq!(bytes[0], b'{');
    }

    #[test]
    fn read_file_bytes_missing_path_errors() {
        let r = read_file_bytes_impl("/nonexistent/definitely-missing.png");
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("No such file"));
    }

    fn wfc_test_project() -> WfcProject {
        WfcProject {
            version: 1,
            slot_w: 1,
            slot_h: 1,
            pieces: vec![WfcPiece {
                id: 7,
                name: "piece".into(),
                tw: 1,
                th: 1,
                tiles: "3".into(),
                weight: 1.0,
                limit: None,
                edge_overrides: Vec::new(),
                tags: Vec::new(),
            }],
            socket_labels: Vec::new(),
            recipes: vec![],
        }
    }

    #[test]
    fn tiled_import_preserves_wfc_and_stamps_while_replacing_map_data() {
        let mut current = GameMap {
            width: 1,
            height: 1,
            tiles: vec![vec![99]],
            npcs: Vec::new(),
            scripts: Vec::new(),
            collision: vec![vec![0]],
            collision_types: Vec::new(),
            encounters: vec![vec![-1]],
            encounter_zones: Vec::new(),
            wfc: Some(wfc_test_project()),
            stamps: vec![Stamp {
                id: 4,
                name: "saved".into(),
                w: 1,
                h: 1,
                tiles: "8".into(),
                origin: None,
            }],
        };
        let expected_wfc = current.wfc.clone();
        let expected_stamps = current.stamps.clone();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/tiled_import.json");

        current = tiled_import_path(path.to_string_lossy().into_owned(), current)
            .expect("Tiled fixture imports");

        assert_eq!((current.width, current.height), (2, 1));
        assert_eq!(current.tiles, vec![vec![7, 8]]);
        assert_eq!(current.scripts[0].script, "msg {imported}");
        assert_eq!(current.npcs[0].script, "@imported");
        assert_eq!(current.wfc, expected_wfc);
        assert_eq!(current.stamps, expected_stamps);
    }

    #[test]
    fn capture_piece_allocates_max_id_plus_one() {
        let mut map = GameMap {
            width: 256,
            height: 256,
            tiles: vec![vec![0; 256]; 256],
            npcs: Vec::new(),
            scripts: Vec::new(),
            collision: Vec::new(),
            collision_types: Vec::new(),
            encounters: Vec::new(),
            encounter_zones: Vec::new(),
            wfc: Some(wfc_test_project()),
            stamps: Vec::new(),
        };
        let existing = map.wfc.as_ref().unwrap().pieces[0].clone();
        map.wfc
            .as_mut()
            .unwrap()
            .pieces
            .push(WfcPiece { id: 19, ..existing });

        let piece = capture_piece(
            map,
            TileRect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            },
            CaptureOpts {
                name: "captured".into(),
                weight: 1.0,
                limit: None,
            },
        )
        .expect("piece capture");
        assert_eq!(piece.id, 20);
        assert_eq!(piece.name, "captured");
    }

    #[test]
    fn wfc_generate_maps_contradiction_with_slot_coordinates() {
        let project = WfcProject {
            version: 1,
            slot_w: 1,
            slot_h: 1,
            pieces: vec![WfcPiece {
                id: 1,
                name: "dead end".into(),
                tw: 1,
                th: 1,
                tiles: "1".into(),
                weight: 1.0,
                limit: None,
                edge_overrides: [
                    EdgeSocket {
                        side: Side::N,
                        index: 0,
                        label: 1,
                    },
                    EdgeSocket {
                        side: Side::E,
                        index: 0,
                        label: 1,
                    },
                    EdgeSocket {
                        side: Side::S,
                        index: 0,
                        label: 1,
                    },
                    EdgeSocket {
                        side: Side::W,
                        index: 0,
                        label: 1,
                    },
                ]
                .into(),
                tags: Vec::new(),
            }],
            socket_labels: vec![
                SocketLabel {
                    id: 1,
                    name: "one-way".into(),
                    kind: SocketKind::Directed { partner: 2 },
                },
                SocketLabel {
                    id: 2,
                    name: "partner".into(),
                    kind: SocketKind::Symmetric,
                },
            ],
            recipes: vec![WfcRecipe {
                id: 1,
                name: "contradiction".into(),
                piece_ids: vec![1],
                size_w: 2,
                size_h: 1,
                seed: 1,
                anchors: Vec::new(),
                socket_compat: Vec::new(),
                rules: Vec::new(),
            }],
        };
        let error = wfc_generate(project, 1, None, Some(1)).expect_err("must contradict");
        assert!(error.contains("contradiction at cell"));
        assert!(error.contains("(1, 0)"));
    }

    #[test]
    fn wfc_generate_maps_memory_budget_with_both_byte_figures() {
        let pieces: Vec<WfcPiece> = (0..257)
            .map(|id| WfcPiece {
                id,
                name: format!("piece-{id}"),
                tw: 1,
                th: 1,
                tiles: "0".into(),
                weight: 1.0,
                limit: None,
                edge_overrides: Vec::new(),
                tags: Vec::new(),
            })
            .collect();
        let project = WfcProject {
            version: 1,
            slot_w: 1,
            slot_h: 1,
            pieces,
            socket_labels: Vec::new(),
            recipes: vec![WfcRecipe {
                id: 1,
                name: "too large".into(),
                piece_ids: (0..257).collect(),
                size_w: 256,
                size_h: 256,
                seed: 1,
                anchors: Vec::new(),
                socket_compat: Vec::new(),
                rules: Vec::new(),
            }],
        };
        let error = wfc_generate(project, 1, None, Some(1)).expect_err("must exceed budget");
        assert!(error.contains("memory budget exceeded"));
        assert!(error.contains("need "));
        assert!(error.contains(" cap "));
    }
}
