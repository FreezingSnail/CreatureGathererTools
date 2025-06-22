use std::fs;

use pokervm_rust::parser::load_from_json;

#[test]
fn parses_script_objects() {
    let json = fs::read_to_string("tests/world_map.json").unwrap();
    let proj = load_from_json(&json).expect("valid json");

    // sample file has two script objects
    assert_eq!(proj.scripts.objects.len(), 3);

    let first = &proj.scripts.objects[0];
    assert_eq!(first.script, "tp @test_teleport @test_house;");
    assert!((first.x - 7.16146).abs() < 1e-1);
    assert!((first.y - 75.6717).abs() < 1e-1);
}
