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
