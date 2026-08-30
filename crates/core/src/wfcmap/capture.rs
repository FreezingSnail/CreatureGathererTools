use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{WfcPiece, encode};

const MAP_SIZE: u32 = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    EmptyRect,
    OutOfBounds,
    ZeroSlotSize,
    RaggedTiles,
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRect => write!(f, "capture rectangle must have non-zero area"),
            Self::OutOfBounds => write!(f, "capture rectangle is outside the 256x256 map"),
            Self::ZeroSlotSize => write!(f, "slot dimensions must be greater than zero"),
            Self::RaggedTiles => write!(f, "source map tiles must be a rectangular 256x256 grid"),
        }
    }
}

impl Error for CaptureError {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureOpts {
    pub name: String,
    pub weight: f32,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Captures a map selection, expanding it to complete WFC slot boundaries.
///
/// Selections must lie on the map. Expansion is deliberately rejected when it
/// would leave the map: truncating the expanded region would create a piece
/// whose dimensions no longer agree with its slot grid.
pub fn capture_piece(
    tiles: &[Vec<u16>],
    rect: TileRect,
    slot_w: u32,
    slot_h: u32,
    next_id: u32,
    opts: &CaptureOpts,
) -> Result<WfcPiece, CaptureError> {
    if slot_w == 0 || slot_h == 0 {
        return Err(CaptureError::ZeroSlotSize);
    }
    if rect.w == 0 || rect.h == 0 {
        return Err(CaptureError::EmptyRect);
    }
    let end_x = rect
        .x
        .checked_add(rect.w)
        .ok_or(CaptureError::OutOfBounds)?;
    let end_y = rect
        .y
        .checked_add(rect.h)
        .ok_or(CaptureError::OutOfBounds)?;
    if rect.x >= MAP_SIZE || rect.y >= MAP_SIZE || end_x > MAP_SIZE || end_y > MAP_SIZE {
        return Err(CaptureError::OutOfBounds);
    }

    let x = (rect.x / slot_w) * slot_w;
    let y = (rect.y / slot_h) * slot_h;
    let snapped_end_x = end_x
        .checked_add(slot_w - 1)
        .ok_or(CaptureError::OutOfBounds)?
        / slot_w
        * slot_w;
    let snapped_end_y = end_y
        .checked_add(slot_h - 1)
        .ok_or(CaptureError::OutOfBounds)?
        / slot_h
        * slot_h;
    if snapped_end_x > MAP_SIZE
        || snapped_end_y > MAP_SIZE
        || snapped_end_x <= x
        || snapped_end_y <= y
    {
        return Err(CaptureError::OutOfBounds);
    }

    if tiles.len() < MAP_SIZE as usize || tiles.iter().any(|row| row.len() < MAP_SIZE as usize) {
        return Err(CaptureError::RaggedTiles);
    }

    let tw = snapped_end_x - x;
    let th = snapped_end_y - y;
    let mut captured = Vec::with_capacity((tw * th) as usize);
    for source_y in y..snapped_end_y {
        captured.extend_from_slice(&tiles[source_y as usize][x as usize..snapped_end_x as usize]);
    }

    Ok(WfcPiece {
        id: next_id,
        name: opts.name.clone(),
        tw,
        th,
        tiles: encode(&captured),
        weight: opts.weight,
        limit: opts.limit,
        edge_overrides: Vec::new(),
        tags: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Vec<Vec<u16>> {
        (0..256)
            .map(|y| (0..256).map(|x| (y * 256 + x) as u16).collect())
            .collect()
    }

    #[test]
    fn snaps_outward() {
        let piece = capture_piece(
            &map(),
            TileRect {
                x: 3,
                y: 5,
                w: 7,
                h: 3,
            },
            2,
            2,
            9,
            &CaptureOpts {
                name: "test".into(),
                weight: 2.0,
                limit: Some(3),
            },
        )
        .expect("capture");
        assert_eq!((piece.tw, piece.th), (8, 4));
        assert_eq!(piece.id, 9);
        assert_eq!(
            super::super::rle::decode(&piece.tiles, 32).unwrap()[0],
            4 * 256 + 2
        );
    }
}
