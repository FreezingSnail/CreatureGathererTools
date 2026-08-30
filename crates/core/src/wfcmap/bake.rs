use serde::{Deserialize, Serialize};

use super::{CompiledRecipe, GenerateError};

/// A generated tile hunk plus the chosen solver symbol for every slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    pub w: u32,
    pub h: u32,
    pub slot_w: u32,
    pub slot_h: u32,
    pub tiles: Vec<u16>,
    pub slots: Vec<u32>,
    pub seed: u64,
    pub attempts: u32,
}

/// Blit solved slot symbols into the row-major tile hunk.
pub fn bake(
    compiled: &CompiledRecipe,
    slots: &[u32],
    seed: u64,
    attempts: u32,
) -> Result<Hunk, GenerateError> {
    if slots.len() != compiled.grid.cells() {
        return Err(GenerateError::Validation(vec![format!(
            "solver returned {} slots, expected {}",
            slots.len(),
            compiled.grid.cells()
        )]));
    }
    let w = compiled
        .grid
        .w
        .checked_mul(compiled.slot_w)
        .ok_or_else(|| GenerateError::Validation(vec!["hunk width overflow".to_string()]))?;
    let h = compiled
        .grid
        .h
        .checked_mul(compiled.slot_h)
        .ok_or_else(|| GenerateError::Validation(vec!["hunk height overflow".to_string()]))?;
    let tile_count = (w as usize)
        .checked_mul(h as usize)
        .ok_or_else(|| GenerateError::Validation(vec!["hunk tile count overflow".to_string()]))?;
    let mut tiles = vec![0; tile_count];
    let slot_tile_count = (compiled.slot_w as usize)
        .checked_mul(compiled.slot_h as usize)
        .ok_or_else(|| GenerateError::Validation(vec!["slot tile count overflow".to_string()]))?;

    for (cell, &symbol) in slots.iter().enumerate() {
        let info = compiled.symbols.get(symbol as usize).ok_or_else(|| {
            GenerateError::Validation(vec![format!("solver returned unknown symbol {symbol}")])
        })?;
        if info.tiles.len() != slot_tile_count {
            return Err(GenerateError::Validation(vec![format!(
                "symbol {symbol} has {} tiles, expected {slot_tile_count}",
                info.tiles.len()
            )]));
        }
        let sx = (cell as u32 % compiled.grid.w) * compiled.slot_w;
        let sy = (cell as u32 / compiled.grid.w) * compiled.slot_h;
        for local_y in 0..compiled.slot_h {
            let source = (local_y * compiled.slot_w) as usize;
            let target = ((sy + local_y) * w + sx) as usize;
            tiles[target..target + compiled.slot_w as usize]
                .copy_from_slice(&info.tiles[source..source + compiled.slot_w as usize]);
        }
    }

    debug_assert_eq!(tiles.len(), (w * h) as usize);
    Ok(Hunk {
        w,
        h,
        slot_w: compiled.slot_w,
        slot_h: compiled.slot_h,
        tiles,
        slots: slots.to_vec(),
        seed,
        attempts,
    })
}
