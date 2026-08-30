use std::collections::{HashMap, HashSet};

use cgfx_wfc::{Dir, Grid, Preset, Rules};

use super::{
    GenerateError, ResolvedSocket, Slice, WfcPiece, WfcProject, WfcRecipe, slice_piece,
    sockets_match,
};

/// A solver symbol and the authored slot content it represents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolInfo {
    pub piece_id: u32,
    pub ox: u32,
    pub oy: u32,
    pub is_anchor: bool,
    pub tiles: Vec<u16>,
}

/// Solver-ready rules, symbols, presets, and per-piece limits for one recipe.
#[derive(Debug, Clone)]
pub struct CompiledRecipe {
    pub rules: Rules,
    pub grid: Grid,
    pub symbols: Vec<SymbolInfo>,
    pub presets: Vec<Preset>,
    pub limits: Vec<(u32, u32)>,
    pub slot_w: u32,
    pub slot_h: u32,
}

/// Compile a recipe using its authored slot dimensions.
pub fn compile_recipe(
    project: &WfcProject,
    recipe: &WfcRecipe,
) -> Result<CompiledRecipe, GenerateError> {
    compile_recipe_for_size(project, recipe, (recipe.size_w, recipe.size_h))
}

pub fn compile_recipe_for_size(
    project: &WfcProject,
    recipe: &WfcRecipe,
    (size_w, size_h): (u32, u32),
) -> Result<CompiledRecipe, GenerateError> {
    // Keep malformed anchor geometry typed: callers can distinguish placement
    // mistakes from the aggregate schema errors for the rest of the project.
    check_anchor_geometry(project, recipe, size_w, size_h)?;
    project.validate().map_err(GenerateError::Validation)?;

    if recipe.piece_ids.is_empty() {
        return Err(GenerateError::NoPieces);
    }
    if project.slot_w == 0 || project.slot_h == 0 {
        return Err(GenerateError::Validation(vec![
            "slot dimensions must be greater than zero".to_string(),
        ]));
    }
    let tile_w = size_w.checked_mul(project.slot_w);
    let tile_h = size_h.checked_mul(project.slot_h);
    if size_w == 0
        || size_h == 0
        || tile_w.is_none_or(|width| width > 256)
        || tile_h.is_none_or(|height| height > 256)
    {
        return Err(GenerateError::Validation(vec![format!(
            "recipe size {size_w}x{size_h} exceeds the 256x256 tile limit"
        )]));
    }

    let pieces: Vec<&WfcPiece> = recipe
        .piece_ids
        .iter()
        .map(|id| {
            project
                .pieces
                .iter()
                .find(|piece| piece.id == *id)
                .ok_or_else(|| {
                    GenerateError::Validation(vec![format!(
                        "recipe {} references unknown piece {}",
                        recipe.id, id
                    )])
                })
        })
        .collect::<Result<_, _>>()?;

    // Reject waves that cannot fit the solver's fixed memory budget before
    // building the quadratic rule matrix. This keeps oversized authoring
    // requests fast and reports the same byte figures as the solver.
    let symbol_count = pieces.iter().try_fold(0usize, |count, piece| {
        let piece_symbols = (piece.tw as usize)
            .checked_div(project.slot_w as usize)
            .and_then(|width| {
                (piece.th as usize)
                    .checked_div(project.slot_h as usize)
                    .and_then(|height| width.checked_mul(height))
            })?;
        count.checked_add(piece_symbols)
    });
    let needed = symbol_count
        .and_then(|symbols| {
            (size_w as usize)
                .checked_mul(size_h as usize)
                .and_then(|cells| cells.checked_mul(symbols))
        })
        .and_then(|entries| entries.checked_mul(4))
        .and_then(|entries| entries.checked_mul(std::mem::size_of::<u16>()))
        .unwrap_or(usize::MAX);
    let cap = cgfx_wfc::Params::seeded(0).max_bytes;
    if needed > cap {
        return Err(GenerateError::Solver(cgfx_wfc::WfcError::MemoryBudget {
            needed,
            cap,
        }));
    }

    for anchor in &recipe.anchors {
        if !recipe.piece_ids.contains(&anchor.piece_id) {
            return Err(GenerateError::Validation(vec![format!(
                "recipe {} anchor references piece {} outside piece_ids",
                recipe.id, anchor.piece_id
            )]));
        }
    }

    let mut symbols = Vec::new();
    let mut slices_by_piece = HashMap::<u32, Vec<(u32, Slice)>>::new();
    for piece in &pieces {
        let slices = slice_piece(piece, project.slot_w, project.slot_h).map_err(|error| {
            GenerateError::Validation(vec![format!(
                "piece {} cannot be sliced: {error}",
                piece.id
            )])
        })?;
        let mut indexed = Vec::with_capacity(slices.len());
        for slice in slices {
            let symbol = symbols.len() as u32;
            symbols.push(SymbolInfo {
                piece_id: slice.piece_id,
                ox: slice.ox,
                oy: slice.oy,
                is_anchor: slice.is_anchor,
                tiles: slice.tiles.clone(),
            });
            indexed.push((symbol, slice));
        }
        slices_by_piece.insert(piece.id, indexed);
    }
    if symbols.is_empty() {
        return Err(GenerateError::NoPieces);
    }

    let mut rules = Rules::new(symbols.len());
    for (symbol, info) in symbols.iter().enumerate() {
        let piece = pieces
            .iter()
            .find(|piece| piece.id == info.piece_id)
            .expect("symbols originate from recipe pieces");
        rules.set_weight(
            symbol as u32,
            if info.is_anchor {
                piece.weight as f64
            } else {
                1.0
            },
        );
    }

    for (a, a_info) in symbols.iter().enumerate() {
        let a_slice = &slices_by_piece[&a_info.piece_id]
            .iter()
            .find(|(symbol, _)| *symbol == a as u32)
            .expect("symbol slice exists")
            .1;
        for (b, b_info) in symbols.iter().enumerate() {
            let b_slice = &slices_by_piece[&b_info.piece_id]
                .iter()
                .find(|(symbol, _)| *symbol == b as u32)
                .expect("symbol slice exists")
                .1;
            for dir in Dir::ALL {
                let left = &a_slice.edges[dir as usize];
                let right = &b_slice.edges[dir.opposite() as usize];
                if same_socket_family(left, right)
                    && sockets_match(left, right, &project.socket_labels, &recipe.socket_compat)
                {
                    rules.allow(a as u32, dir, b as u32);
                }
            }
        }
    }

    let grid = Grid::new(size_w, size_h);
    let mut presets = Vec::new();
    let mut limits = Vec::new();
    for piece in &pieces {
        if let Some(limit) = piece.limit
            && let Some((symbol, _)) = slices_by_piece[&piece.id]
                .iter()
                .find(|(_, slice)| slice.is_anchor)
        {
            limits.push((*symbol, limit));
        }
    }
    for anchor in &recipe.anchors {
        let slices = &slices_by_piece[&anchor.piece_id];
        for (symbol, slice) in slices {
            let sx = anchor.sx + slice.ox / project.slot_w;
            let sy = anchor.sy + slice.oy / project.slot_h;
            presets.push(Preset {
                cell: grid.index(sx, sy),
                allowed: vec![*symbol],
            });
        }
    }

    Ok(CompiledRecipe {
        rules,
        grid,
        symbols,
        presets,
        limits,
        slot_w: project.slot_w,
        slot_h: project.slot_h,
    })
}

fn same_socket_family(left: &ResolvedSocket, right: &ResolvedSocket) -> bool {
    matches!(
        (left, right),
        (ResolvedSocket::Private(_), ResolvedSocket::Private(_))
            | (ResolvedSocket::Strip(_), ResolvedSocket::Strip(_))
            | (ResolvedSocket::Label(_), ResolvedSocket::Label(_))
    )
}

fn check_anchor_geometry(
    project: &WfcProject,
    recipe: &WfcRecipe,
    size_w: u32,
    size_h: u32,
) -> Result<(), GenerateError> {
    let mut occupied = HashSet::new();
    for anchor in &recipe.anchors {
        let Some(piece) = project
            .pieces
            .iter()
            .find(|piece| piece.id == anchor.piece_id)
        else {
            continue;
        };
        if project.slot_w == 0 || project.slot_h == 0 {
            continue;
        }
        let width = piece.tw / project.slot_w;
        let height = piece.th / project.slot_h;
        let fits = anchor
            .sx
            .checked_add(width)
            .is_some_and(|end| end <= size_w)
            && anchor
                .sy
                .checked_add(height)
                .is_some_and(|end| end <= size_h);
        if !fits {
            return Err(GenerateError::AnchorOutOfBounds {
                piece_id: anchor.piece_id,
                sx: anchor.sx,
                sy: anchor.sy,
            });
        }
        for sy in anchor.sy..anchor.sy + height {
            for sx in anchor.sx..anchor.sx + width {
                if !occupied.insert((sx, sy)) {
                    return Err(GenerateError::AnchorOverlap { sx, sy });
                }
            }
        }
    }
    Ok(())
}
