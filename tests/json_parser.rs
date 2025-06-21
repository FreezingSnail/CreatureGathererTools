use std::fs;

use pokervm_rust::parser::load_from_json;

#[test]
fn parses_script_objects() {
    let json = fs::read_to_string("testdata/world_map.json").unwrap();
    let proj = load_from_json(&json).expect("valid json");

    // sample file has two script objects
    assert_eq!(proj.scripts.objects.len(), 2);

    let first = &proj.scripts.objects[0];
    assert_eq!(first.script, "tp @test_teleport @test_house;");
    assert!((first.x - 7.8261).abs() < 1e-3);
    assert!((first.y - 75.3462).abs() < 1e-3);
}
