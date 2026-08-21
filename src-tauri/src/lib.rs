//! Tauri 2 command layer: IPC wrappers over `creaturevm` (cgfx-core).
//!
//! Every command returns a structured, serializable value or
//! `Result<T, String>` — no panic across IPC, no clap in this layer.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Mutex;

use creaturevm::builder;
use creaturevm::config::Config;
use creaturevm::model::{Creature, GameMap, Move};
use creaturevm::processor::ast::{Branch, Cmd, Condition, Location};
use creaturevm::script::{self, Controller};
use creaturevm::store::Store;
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
            }
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
            }
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

// ─────────────────────────────────────────────────────────────────────
// Tiled JSON interchange
// ─────────────────────────────────────────────────────────────────────

#[tauri::command]
fn tiled_import_path(path: String) -> Result<GameMap, String> {
    let data = std::fs::read_to_string(&path).map_err(err_str)?;
    creaturevm::tiled::import(&data).map_err(|e| e.0)
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
            parse_script,
            read_file_bytes,
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
}
