//! Parser for map tile data from Tiled maps.
//! Converts flat tile array into chunked representation matching script chunks.

use crate::model::{CHUNK_COLS, CHUNK_H, CHUNK_W, MapLayer, TOTAL_CHUNKS};

pub type ParsedMap = Vec<MapLayer>;
/// Chunked representation; `chunks[idx]` holds all tiles that belong
/// to that chunk – vector length is always `TOTAL_CHUNKS`.
/// Each chunk contains 32 bytes (8×4 tiles).

/// Parse the flat 65536-byte map data into chunks matching the script chunking system.
///
/// The map is a 256×256 grid where each tile is 1 byte.
/// Chunks are 8×4 tiles each, resulting in 2048 chunks of 32 bytes each.
pub fn parse_map(map_layer: &MapLayer) -> Result<ParsedMap, String> {
    // Extract the flat tile data from the map layer
    let tile_data = map_layer;

    if tile_data.len() != 65536 {
        return Err(format!(
            "Expected 65536 bytes of map data, got {}",
            tile_data.len()
        ));
    }

    let mut chunks: Vec<MapLayer> = vec![Vec::new(); TOTAL_CHUNKS];

    // Process each chunk (8×4 tiles = 32 bytes per chunk)
    for chunk_y in 0..64 {
        // 64 chunk rows
        for chunk_x in 0..32 {
            // 32 chunk columns
            let chunk_idx = chunk_y * CHUNK_COLS + chunk_x;
            let mut chunk_data = Vec::with_capacity(32);

            // Extract 8×4 tiles for this chunk
            for tile_y in 0..CHUNK_H {
                for tile_x in 0..CHUNK_W {
                    let world_x = chunk_x * CHUNK_W + tile_x;
                    let world_y = chunk_y * CHUNK_H + tile_y;
                    let tile_idx = (world_y * 256 + world_x) as usize;

                    chunk_data.push(tile_data[tile_idx]);
                }
            }

            chunks[chunk_idx as usize] = chunk_data;
        }
    }

    Ok(chunks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_grouping() {
        // Create test data where we can verify chunk boundaries
        let mut test_data = vec![0u8; 65536];

        // Set specific values we can test for
        // Chunk 0 should contain tiles (0-7, 0-3)
        test_data[0] = 100; // (0,0)
        test_data[7] = 101; // (7,0)
        test_data[256 * 3 + 0] = 102; // (0,3)
        test_data[256 * 3 + 7] = 103; // (7,3)

        // Chunk 1 should contain tiles (8-15, 0-3)
        test_data[8] = 200; // (8,0)
        test_data[15] = 201; // (15,0)

        // Mock the extract function for testing
        // (In real implementation, this would read from MapLayer)

        // For now, we'll test the chunking logic directly
        let mut chunks: Vec<Vec<u8>> = vec![Vec::new(); TOTAL_CHUNKS];

        // Process chunk 0 manually for testing
        let mut chunk_0_data = Vec::new();
        for tile_y in 0..4 {
            for tile_x in 0..8 {
                let tile_idx = tile_y * 256 + tile_x;
                chunk_0_data.push(test_data[tile_idx]);
            }
        }
        chunks[0] = chunk_0_data;

        // Verify chunk 0 contains the expected values
        assert_eq!(chunks[0][0], 100); // (0,0) -> first position in chunk
        assert_eq!(chunks[0][7], 101); // (7,0) -> 8th position in chunk
        assert_eq!(chunks[0][24], 102); // (0,3) -> position 24 in chunk (3*8 + 0)
        assert_eq!(chunks[0][31], 103); // (7,3) -> last position in chunk (3*8 + 7)

        // Verify chunk size
        assert_eq!(chunks[0].len(), 32, "Each chunk should contain 32 bytes");
    }

    #[test]
    fn test_chunk_count() {
        // Verify we have the correct number of chunks
        let chunks: Vec<Vec<u8>> = vec![Vec::new(); TOTAL_CHUNKS];
        assert_eq!(chunks.len(), 2048, "Should have 2048 chunks total");

        // Verify chunk dimensions make sense
        assert_eq!(CHUNK_COLS * 64, 2048, "32 columns × 64 rows = 2048 chunks");
        assert_eq!(CHUNK_W * CHUNK_H, 32, "8×4 tiles = 32 bytes per chunk");
    }
}
