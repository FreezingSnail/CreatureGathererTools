use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use creaturevm::model::GameMap;

const WFC_FIXTURE: &str = include_str!("testdata/wfc/project.json");
static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestWorkspace {
    path: PathBuf,
}

impl TestWorkspace {
    fn new() -> Self {
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/cli-wfc-tests")
            .join(format!("{}-{sequence}", std::process::id()));
        fs::create_dir_all(&path).expect("create test workspace under target");
        Self { path }
    }

    fn map_path(&self) -> PathBuf {
        self.path.join("map.json")
    }

    fn write_map(&self, map: &GameMap) {
        fs::write(
            self.map_path(),
            serde_json::to_string_pretty(map).expect("serialize map fixture"),
        )
        .expect("write map fixture");
    }
}

fn fixture_map() -> GameMap {
    GameMap {
        width: 2,
        height: 2,
        tiles: vec![vec![1, 2], vec![3, 4]],
        tilesets: vec![],
        tile_properties: Default::default(),
        source_path: None,
        npcs: vec![],
        scripts: vec![],
        collision: vec![],
        collision_types: vec![],
        encounters: vec![],
        encounter_zones: vec![],
        wfc: Some(serde_json::from_str(WFC_FIXTURE).expect("WFC fixture parses")),
        stamps: vec![],
    }
}

fn run(workspace: &TestWorkspace, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cgfx-tools"))
        .current_dir(&workspace.path)
        .args(args)
        .output()
        .expect("run cgfx-tools")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).expect("command output is UTF-8")
}

#[test]
fn fixed_seed_writes_byte_identical_hunk_dump() {
    let workspace = TestWorkspace::new();
    let map = fixture_map();
    workspace.write_map(&map);
    let output = workspace.path.join("hunk.json");
    let map = workspace.map_path();
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map.to_str().unwrap(),
            "--recipe",
            "alternating",
            "--seed",
            "7",
            "--out",
            output.to_str().unwrap(),
        ],
    );

    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(text(&result.stdout).contains("recipe alternating; seed 7; attempts 1"));
    assert_eq!(
        fs::read(&output).expect("hunk dump"),
        br#"{
  "w": 4,
  "h": 3,
  "slot_w": 1,
  "slot_h": 1,
  "tiles": "1,2,1,2x2,1,2,1x2,2,1,2",
  "seed": 7,
  "attempts": 1
}"#
    );
}

#[test]
fn dry_run_prints_piece_names_without_writing() {
    let workspace = TestWorkspace::new();
    let map = fixture_map();
    workspace.write_map(&map);
    let map_path = workspace.map_path();
    let before = fs::read(&map_path).expect("original map bytes");
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map_path.to_str().unwrap(),
            "--recipe",
            "1",
            "--dry-run",
        ],
    );

    assert!(result.status.success(), "{}", text(&result.stderr));
    assert_eq!(fs::read(&map_path).unwrap(), before, "dry run changed map");
    let stdout = text(&result.stdout);
    assert!(stdout.contains("slot grid:"));
    assert!(stdout.contains("one"));
    assert!(stdout.contains("two"));
    assert!(stdout.contains("piece counts:"));
}

#[test]
fn in_place_appends_one_stamp_and_preserves_every_other_map_field() {
    let workspace = TestWorkspace::new();
    let original = fixture_map();
    workspace.write_map(&original);
    let map_path = workspace.map_path();
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map_path.to_str().unwrap(),
            "--recipe",
            "alternating",
            "--in-place",
            "--stamp-name",
            "cli stamp",
        ],
    );

    assert!(result.status.success(), "{}", text(&result.stderr));
    let saved = fs::read_to_string(&map_path).expect("saved map");
    assert!(
        saved.contains("\n  \"width\":"),
        "map must use 2-space JSON"
    );
    let mut actual: GameMap = serde_json::from_str(&saved).expect("reparse saved map");
    assert_eq!(actual.stamps.len(), 1);
    let stamp = actual.stamps.pop().unwrap();
    assert_eq!(stamp.id, 1);
    assert_eq!(stamp.name, "cli stamp");
    assert_eq!((stamp.w, stamp.h), (4, 3));
    assert_eq!(stamp.tiles, "1,2,1,2x2,1,2,1x2,2,1,2");
    assert_eq!(stamp.origin.unwrap().recipe_id, 1);
    assert_eq!(actual, original, "only stamps may differ");
}

#[test]
fn missing_output_mode_exits_two_and_names_both_flags() {
    let workspace = TestWorkspace::new();
    workspace.write_map(&fixture_map());
    let map = workspace.map_path();
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map.to_str().unwrap(),
            "--recipe",
            "alternating",
        ],
    );

    assert_eq!(result.status.code(), Some(2));
    let stderr = text(&result.stderr);
    assert!(stderr.contains("--out"));
    assert!(stderr.contains("--in-place"));
}

#[test]
fn unknown_recipe_exits_two_and_lists_available_recipe_names() {
    let workspace = TestWorkspace::new();
    workspace.write_map(&fixture_map());
    let map = workspace.map_path();
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map.to_str().unwrap(),
            "--recipe",
            "missing",
            "--out",
            workspace.path.join("hunk.json").to_str().unwrap(),
        ],
    );

    assert_eq!(result.status.code(), Some(2));
    assert!(text(&result.stderr).contains("available recipes: alternating"));
}

#[test]
fn contradictory_recipe_exits_three_with_slot_coordinates() {
    let workspace = TestWorkspace::new();
    let mut map = fixture_map();
    map.wfc.as_mut().unwrap().recipes[0].piece_ids = vec![1];
    workspace.write_map(&map);
    let map = workspace.map_path();
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map.to_str().unwrap(),
            "--recipe",
            "alternating",
            "--out",
            workspace.path.join("hunk.json").to_str().unwrap(),
        ],
    );

    assert_eq!(result.status.code(), Some(3));
    let stderr = text(&result.stderr);
    assert!(stderr.contains("contradiction at cell"));
    assert!(stderr.contains('('));
}

#[test]
fn limit_failure_exits_four_with_piece_and_limit_detail() {
    let workspace = TestWorkspace::new();
    let mut map = fixture_map();
    map.wfc.as_mut().unwrap().pieces[0].limit = Some(0);
    workspace.write_map(&map);
    let map = workspace.map_path();
    let result = run(
        &workspace,
        &[
            "wfc",
            "generate",
            "--map",
            map.to_str().unwrap(),
            "--recipe",
            "alternating",
            "--out",
            workspace.path.join("hunk.json").to_str().unwrap(),
        ],
    );

    assert_eq!(result.status.code(), Some(4));
    assert!(text(&result.stderr).contains("piece 1 limit 0 unsatisfied after 10 attempts"));
}
