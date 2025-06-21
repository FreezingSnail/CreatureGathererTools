//! Parser for location entries from Tiled maps.
//! Converts location objects into a lookup table for script resolution.

use crate::model::LocationLayer;
use std::collections::HashMap;

pub type LocationTags = HashMap<String, (u16, u16)>;

/// Parse location entries into a name -> (x, y) coordinate lookup table.
///
/// Each location's world coordinates are divided by 16 to convert from
/// pixel coordinates to tile coordinates.
pub fn parse_locations(locations: &LocationLayer) -> LocationTags {
    let mut location_map = HashMap::new();

    for location in &locations.objects {
        let tile_x = (location.x / 16.0) as u16;
        let tile_y = (location.y / 16.0) as u16;

        // remove the leading @ symbol on the name string
        let name = location.name.strip_prefix('@').unwrap_or(&location.name);
        location_map.insert(name.to_string(), (tile_x, tile_y));
    }

    location_map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{LocationEntry, LocationLayer};

    #[test]
    fn test_parse_locations() {
        let layer = LocationLayer {
            objects: vec![
                LocationEntry {
                    id: 1,
                    name: "spawn".to_string(),
                    x: 32.0, // 32 / 16 = 2
                    y: 48.0, // 48 / 16 = 3
                },
                LocationEntry {
                    id: 2,
                    name: "shop".to_string(),
                    x: 160.0, // 160 / 16 = 10
                    y: 80.0,  // 80 / 16 = 5
                },
            ],
        };

        let locations = parse_locations(&layer);

        assert_eq!(locations.len(), 2);
        assert_eq!(locations.get("spawn"), Some(&(2, 3)));
        assert_eq!(locations.get("shop"), Some(&(10, 5)));
    }

    #[test]
    fn test_parse_locations_empty() {
        let layer = LocationLayer { objects: vec![] };

        let locations = parse_locations(&layer);
        assert_eq!(locations.len(), 0);
    }

    #[test]
    fn test_parse_locations_fractional_coordinates() {
        let layer = LocationLayer {
            objects: vec![LocationEntry {
                id: 1,
                name: "test".to_string(),
                x: 33.7, // 33.7 / 16 = 2.10625 -> 2
                y: 47.9, // 47.9 / 16 = 2.99375 -> 2
            }],
        };

        let locations = parse_locations(&layer);
        assert_eq!(locations.get("test"), Some(&(2, 2)));
    }
}
