use creaturevm::manifest::Manifest;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const TESTDATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata");
const PROJECT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/testdata/manifest-project.json"
);
const OUTPUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/builder_out/manifest");

fn load_manifest() -> Manifest {
    serde_json::from_slice(&fs::read(format!("{OUTPUT}/manifest.json")).expect("manifest emits"))
        .expect("manifest parses")
}

fn names_on_disk() -> BTreeSet<String> {
    fs::read_dir(OUTPUT)
        .expect("output directory exists")
        .map(|entry| {
            entry
                .expect("read output entry")
                .file_name()
                .into_string()
                .expect("utf8 name")
        })
        .filter(|name| name != "manifest.json")
        .collect()
}

fn artifact_file(artifact_path: &str) -> PathBuf {
    let artifact_path = Path::new(artifact_path);
    if artifact_path.is_absolute() {
        artifact_path.to_owned()
    } else {
        Path::new(OUTPUT).join(artifact_path)
    }
}

fn output_artifact_names(manifest: &Manifest) -> BTreeSet<String> {
    manifest
        .artifacts
        .iter()
        .filter(|artifact| Path::new(&artifact.path).is_relative())
        .map(|artifact| artifact.path.clone())
        .collect()
}

fn arena_temp_project() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("cgfx-arena-manifest-{stamp}"));
    std::fs::create_dir_all(root.join("data")).expect("create project data");
    let config = root.join("creature-tools.json");
    let arena = root.join("arena/arena-demo.toml");
    std::fs::create_dir_all(arena.parent().unwrap()).expect("create arena source directory");
    let font = arena.parent().unwrap().join("font.png");

    std::fs::write(
        &config,
        format!(
            r#"{{
                "creatures_json": "{}/creatures.json",
                "moves_json": "{}/moves.json",
                "map_json": "map.json",
                "tileset_png": "",
                "map_tileset_png": "",
                "npc_tileset_png": "",
                "output_dir": "generated",
                "arena_demo": "arena/arena-demo.toml"
            }}"#,
            TESTDATA, TESTDATA
        ),
    )
    .expect("write project config");
    std::fs::write(
        root.join("map.json"),
        r#"{"width":1,"height":1,"tiles":[[0]],"npcs":[],"scripts":[]}"#,
    )
    .expect("write empty map");
    std::fs::write(
        root.join("data/opponents.csv"),
        "c1,c2,c3,l1,l2,l3,name,c1m1,c1m2,c1m3,c1m4,c2m1,c2m2,c2m3,c2m4,c3m1,c3m2,c3m3,c3m4\n\
         squibblesnail,squibblesnail,squibblesnail,31,31,31,fixture,smite,none,none,none,smite,none,none,none,smite,none,none,none\n",
    )
    .expect("write opponents CSV");
    write_arena_source(&arena, "Starter");
    write_test_font(&font, false);
    (root, config, arena, font)
}

fn write_arena_source(path: &Path, label: &str) {
    std::fs::write(
        path,
        format!(
            r#"label_font = "font.png"

[[player_team]]
key = "starter"
label = "{label}"

[[player_team.members]]
species = "SquibbleSnail"
level = 31
moves = ["smite", "none", "none", "none"]

[[player_team.members]]
species = "SquibbleSnail"
level = 31
moves = ["smite", "none", "none", "none"]

[[player_team.members]]
species = "SquibbleSnail"
level = 31
moves = ["smite", "none", "none", "none"]

[[opponent_team]]
key = "rival"
label = "Rival"
trainer_species = ["SquibbleSnail", "SquibbleSnail", "SquibbleSnail"]
"#
        ),
    )
    .expect("write arena catalog");
}

fn write_test_font(path: &Path, changed_pixel: bool) {
    let mut encoder = png::Encoder::new(std::fs::File::create(path).unwrap(), 640, 6);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("font PNG header");
    let mut pixels = vec![255; 640 * 6 * 4];
    let pixel = (u32::from(if changed_pixel { b't' } else { b'S' }) * 5 * 4) as usize;
    pixels[pixel..pixel + 3].copy_from_slice(&[0, 0, 0]);
    writer.write_image_data(&pixels).expect("font PNG pixels");
}

fn artifact<'a>(manifest: &'a Manifest, path: &Path) -> &'a creaturevm::manifest::Artifact {
    manifest
        .artifacts
        .iter()
        .find(|item| Path::new(&item.path) == path)
        .unwrap_or_else(|| panic!("missing manifest artifact {}", path.display()))
}

#[test]
fn full_project_manifest_lists_complete_hashed_regenerated_artifact_inventory() {
    creaturevm::run_project(PROJECT).expect("full project generation succeeds");
    let manifest = load_manifest();

    assert_eq!(
        manifest.tool_version,
        format!("cgfx-tools {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(
        !manifest.generated_at.is_empty(),
        "generation timestamp is recorded"
    );
    assert_eq!(
        output_artifact_names(&manifest),
        names_on_disk(),
        "manifest lists every output-directory artifact"
    );

    let canonical = Path::new(TESTDATA)
        .join("fxdata/data/creatureData.txt")
        .canonicalize()
        .expect("canonical artifact emits");
    assert!(
        manifest
            .artifacts
            .iter()
            .any(|artifact| Path::new(&artifact.path) == canonical),
        "manifest lists canonical artifact written outside output directory"
    );

    for artifact in &manifest.artifacts {
        let contents = fs::read(artifact_file(&artifact.path))
            .unwrap_or_else(|error| panic!("read {}: {error}", artifact.path));
        assert_eq!(
            artifact.bytes,
            contents.len() as u64,
            "{} byte count",
            artifact.path
        );
        assert_eq!(
            artifact.sha256,
            format!("{:x}", Sha256::digest(contents)),
            "{} digest",
            artifact.path
        );
        assert!(
            Path::new(&artifact.source).starts_with(TESTDATA),
            "{} source path",
            artifact.path
        );
    }

    fs::write(format!("{OUTPUT}/manifest.json"), "stale manifest").expect("corrupt manifest");
    creaturevm::run_project(PROJECT).expect("full project regeneration succeeds");
    let regenerated = load_manifest();
    assert_eq!(
        output_artifact_names(&regenerated),
        names_on_disk(),
        "regeneration replaces rather than appends manifest content",
    );
}

#[test]
fn arena_sources_and_generated_outputs_are_manifested_with_changing_hashes() {
    let (root, config, arena_source, font_source) = arena_temp_project();
    creaturevm::run_project(&config).expect("arena project generation");
    let output = root.join("generated");
    let read_manifest = || {
        serde_json::from_slice::<Manifest>(
            &fs::read(output.join("manifest.json")).expect("arena manifest emits"),
        )
        .expect("arena manifest parses")
    };

    let first = read_manifest();
    let descriptor = artifact(&first, Path::new("arena_demo.txt"));
    let ids = artifact(&first, Path::new("arena_demo_ids.hpp"));
    let font = artifact(&first, &font_source);
    assert_eq!(descriptor.source, arena_source.to_string_lossy());
    assert_eq!(ids.source, arena_source.to_string_lossy());
    assert_eq!(font.source, arena_source.to_string_lossy());
    let original_descriptor_hash = descriptor.sha256.clone();
    let original_font_hash = font.sha256.clone();

    write_arena_source(&arena_source, "Start");
    creaturevm::run_project(&config).expect("regenerate changed arena source");
    let changed_catalog = read_manifest();
    let changed_descriptor_hash = artifact(&changed_catalog, Path::new("arena_demo.txt"))
        .sha256
        .clone();
    assert_ne!(changed_descriptor_hash, original_descriptor_hash);
    assert_eq!(
        artifact(&changed_catalog, &font_source).sha256,
        original_font_hash
    );

    write_test_font(&font_source, true);
    creaturevm::run_project(&config).expect("regenerate changed arena font");
    let changed_font = read_manifest();
    assert_ne!(
        artifact(&changed_font, &font_source).sha256,
        original_font_hash
    );
    assert_ne!(
        artifact(&changed_font, Path::new("arena_demo.txt")).sha256,
        changed_descriptor_hash
    );
    fs::remove_dir_all(root).expect("remove temp project");
}
