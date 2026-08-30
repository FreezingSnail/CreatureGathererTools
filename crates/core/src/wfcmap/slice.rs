use std::error::Error;
use std::fmt;

use super::{RleError, Side, WfcPiece, decode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SliceError {
    ZeroSlotSize,
    TileDimensionsOverflow,
    InvalidTileData(RleError),
    DimensionsNotMultiple {
        tw: u32,
        th: u32,
        slot_w: u32,
        slot_h: u32,
    },
    InvalidOverride {
        side: Side,
        index: u32,
    },
    DuplicateOverride {
        side: Side,
        index: u32,
    },
    InternalOrdinalOverflow,
}

impl fmt::Display for SliceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroSlotSize => write!(f, "slot dimensions must be greater than zero"),
            Self::TileDimensionsOverflow => write!(f, "piece tile dimensions overflow"),
            Self::InvalidTileData(error) => write!(f, "invalid piece tile data: {error}"),
            Self::DimensionsNotMultiple {
                tw,
                th,
                slot_w,
                slot_h,
            } => write!(
                f,
                "piece dimensions {tw}x{th} are not multiples of slot {slot_w}x{slot_h}"
            ),
            Self::InvalidOverride { side, index } => write!(
                f,
                "edge override index is out of range or targets a non-boundary slice: {side:?}[{index}]"
            ),
            Self::DuplicateOverride { side, index } => {
                write!(f, "duplicate edge override {side:?}[{index}]")
            }
            Self::InternalOrdinalOverflow => write!(f, "internal socket ordinal overflow"),
        }
    }
}

impl Error for SliceError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slice {
    pub piece_id: u32,
    pub ox: u32,
    pub oy: u32,
    pub tiles: Vec<u16>,
    pub edges: [ResolvedSocket; 4],
    pub is_anchor: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSocket {
    Private(u64),
    Strip(Vec<u16>),
    Label(u32),
}

/// Slices a variable-size piece into row-major slot cells.
pub fn slice_piece(piece: &WfcPiece, slot_w: u32, slot_h: u32) -> Result<Vec<Slice>, SliceError> {
    if slot_w == 0 || slot_h == 0 {
        return Err(SliceError::ZeroSlotSize);
    }
    if piece.tw == 0
        || piece.th == 0
        || !piece.tw.is_multiple_of(slot_w)
        || !piece.th.is_multiple_of(slot_h)
    {
        return Err(SliceError::DimensionsNotMultiple {
            tw: piece.tw,
            th: piece.th,
            slot_w,
            slot_h,
        });
    }
    let tile_count = (piece.tw as usize)
        .checked_mul(piece.th as usize)
        .ok_or(SliceError::TileDimensionsOverflow)?;
    let tiles = decode(&piece.tiles, tile_count).map_err(SliceError::InvalidTileData)?;
    validate_edge_overrides(piece, slot_w, slot_h)?;

    let cols = piece.tw / slot_w;
    let rows = piece.th / slot_h;
    let grid = SliceGrid {
        cols,
        rows,
        slot_w,
        slot_h,
    };
    let mut slices = Vec::with_capacity((cols as usize).saturating_mul(rows as usize));
    for row in 0..rows {
        for col in 0..cols {
            let ox = col * slot_w;
            let oy = row * slot_h;
            let mut slice_tiles = Vec::with_capacity((slot_w * slot_h) as usize);
            for local_y in 0..slot_h {
                let start = ((oy + local_y) * piece.tw + ox) as usize;
                slice_tiles.extend_from_slice(&tiles[start..start + slot_w as usize]);
            }

            let edges = [
                resolve_edge(piece, &slice_tiles, col, row, Side::N, &grid)?,
                resolve_edge(piece, &slice_tiles, col, row, Side::E, &grid)?,
                resolve_edge(piece, &slice_tiles, col, row, Side::S, &grid)?,
                resolve_edge(piece, &slice_tiles, col, row, Side::W, &grid)?,
            ];
            slices.push(Slice {
                piece_id: piece.id,
                ox,
                oy,
                tiles: slice_tiles,
                edges,
                is_anchor: ox == 0 && oy == 0,
            });
        }
    }
    Ok(slices)
}

/// Validates that every override identifies an actual boundary slot.
///
/// `index` is a slot coordinate along the selected side, not a tile coordinate.
pub fn validate_edge_overrides(
    piece: &WfcPiece,
    slot_w: u32,
    slot_h: u32,
) -> Result<(), SliceError> {
    if slot_w == 0 || slot_h == 0 {
        return Err(SliceError::ZeroSlotSize);
    }
    let cols = piece.tw / slot_w;
    let rows = piece.th / slot_h;
    let mut seen = Vec::with_capacity(piece.edge_overrides.len());
    for edge in &piece.edge_overrides {
        let edge_len = match edge.side {
            Side::N | Side::S => cols,
            Side::E | Side::W => rows,
        };
        if edge.index >= edge_len {
            return Err(SliceError::InvalidOverride {
                side: edge.side,
                index: edge.index,
            });
        }
        if seen
            .iter()
            .any(|(side, index)| *side == edge.side && *index == edge.index)
        {
            return Err(SliceError::DuplicateOverride {
                side: edge.side,
                index: edge.index,
            });
        }
        seen.push((edge.side, edge.index));
    }
    Ok(())
}

struct SliceGrid {
    cols: u32,
    rows: u32,
    slot_w: u32,
    slot_h: u32,
}

fn resolve_edge(
    piece: &WfcPiece,
    tiles: &[u16],
    col: u32,
    row: u32,
    side: Side,
    grid: &SliceGrid,
) -> Result<ResolvedSocket, SliceError> {
    let index = match side {
        Side::N | Side::S => col,
        Side::E | Side::W => row,
    };
    if let Some(override_socket) = piece
        .edge_overrides
        .iter()
        .find(|edge| edge.side == side && edge.index == index)
    {
        return Ok(ResolvedSocket::Label(override_socket.label));
    }

    let internal = match side {
        Side::N if row > 0 => Some((col, row - 1, Side::S)),
        Side::E if col + 1 < grid.cols => Some((col, row, Side::E)),
        Side::S if row + 1 < grid.rows => Some((col, row, Side::S)),
        Side::W if col > 0 => Some((col - 1, row, Side::E)),
        _ => None,
    };
    if let Some((owner_col, owner_row, owner_side)) = internal {
        return Ok(ResolvedSocket::Private(internal_socket_id(
            piece.id, owner_col, owner_row, owner_side, grid.cols,
        )?));
    }

    Ok(ResolvedSocket::Strip(boundary_strip(
        tiles,
        side,
        grid.slot_w,
        grid.slot_h,
    )))
}

fn internal_socket_id(
    piece_id: u32,
    col: u32,
    row: u32,
    side: Side,
    cols: u32,
) -> Result<u64, SliceError> {
    let direction = match side {
        Side::E => 0u64,
        Side::S => 1,
        _ => unreachable!("internal edge owner is E or S"),
    };
    let edge_index = (row as u64)
        .checked_mul(cols as u64)
        .and_then(|value| value.checked_add(col as u64))
        .and_then(|value| value.checked_mul(2))
        .and_then(|value| value.checked_add(direction))
        .ok_or(SliceError::InternalOrdinalOverflow)?;
    if edge_index > u32::MAX as u64 {
        return Err(SliceError::InternalOrdinalOverflow);
    }
    Ok(((piece_id as u64) << 32) | edge_index)
}

fn boundary_strip(tiles: &[u16], side: Side, slot_w: u32, slot_h: u32) -> Vec<u16> {
    match side {
        Side::N => tiles[..slot_w as usize].to_vec(),
        Side::E => (0..slot_h)
            .map(|row| tiles[(row * slot_w + slot_w - 1) as usize])
            .collect(),
        Side::S => tiles[((slot_h - 1) * slot_w) as usize..(slot_h * slot_w) as usize].to_vec(),
        Side::W => (0..slot_h)
            .map(|row| tiles[(row * slot_w) as usize])
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wfcmap::encode;

    fn piece() -> WfcPiece {
        let tiles: Vec<u16> = (0..24).collect();
        WfcPiece {
            id: 7,
            name: "asymmetric".into(),
            tw: 6,
            th: 4,
            tiles: encode(&tiles),
            weight: 1.0,
            limit: None,
            edge_overrides: Vec::new(),
            tags: Vec::new(),
        }
    }

    #[test]
    fn internal_halves_share_private_socket() {
        let slices = slice_piece(&piece(), 2, 2).unwrap();
        assert_eq!(slices[0].edges[1], slices[1].edges[3]);
        assert_eq!(slices[0].edges[2], slices[3].edges[0]);
    }

    #[test]
    fn strips_keep_shared_cell_order() {
        let slices = slice_piece(&piece(), 2, 2).unwrap();
        assert_eq!(slices[0].edges[0], ResolvedSocket::Strip(vec![0, 1]));
        assert_eq!(slices[0].edges[3], ResolvedSocket::Strip(vec![0, 6]));
        assert_eq!(slices[2].edges[0], ResolvedSocket::Strip(vec![4, 5]));
        assert_eq!(slices[2].edges[1], ResolvedSocket::Strip(vec![5, 11]));
    }
}
