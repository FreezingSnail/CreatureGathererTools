use std::error::Error;
use std::fmt;

use cgfx_wfc::{Params, WfcError, solve};

use super::bake::{Hunk, bake};
use super::compile::compile_recipe_for_size;
use super::{ValidationIssue, WfcProject};

/// Failure while compiling or generating a WFC hunk.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerateError {
    UnknownRecipe(u32),
    NoPieces,
    Validation(Vec<ValidationIssue>),
    AnchorOutOfBounds {
        piece_id: u32,
        sx: u32,
        sy: u32,
    },
    AnchorOverlap {
        sx: u32,
        sy: u32,
    },
    LimitUnsatisfied {
        piece_id: u32,
        limit: u32,
        attempts: u32,
    },
    Solver(WfcError),
}

impl fmt::Display for GenerateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRecipe(id) => write!(f, "unknown WFC recipe {id}"),
            Self::NoPieces => write!(f, "WFC recipe contains no pieces"),
            Self::Validation(issues) => write!(f, "invalid WFC project: {}", issues.join("; ")),
            Self::AnchorOutOfBounds { piece_id, sx, sy } => {
                write!(
                    f,
                    "anchor for piece {piece_id} at ({sx}, {sy}) is out of bounds"
                )
            }
            Self::AnchorOverlap { sx, sy } => write!(f, "anchors overlap at slot ({sx}, {sy})"),
            Self::LimitUnsatisfied {
                piece_id,
                limit,
                attempts,
            } => write!(
                f,
                "piece {piece_id} limit {limit} unsatisfied after {attempts} attempts"
            ),
            Self::Solver(error) => write!(f, "WFC solver failed: {error}"),
        }
    }
}

impl Error for GenerateError {}

impl From<WfcError> for GenerateError {
    fn from(error: WfcError) -> Self {
        Self::Solver(error)
    }
}

/// Generate a deterministic tile hunk for a project recipe.
pub fn generate(
    project: &WfcProject,
    recipe_id: u32,
    size_override: Option<(u32, u32)>,
    seed_override: Option<u64>,
) -> Result<Hunk, GenerateError> {
    let recipe = project
        .recipes
        .iter()
        .find(|recipe| recipe.id == recipe_id)
        .ok_or(GenerateError::UnknownRecipe(recipe_id))?;
    let size = size_override.unwrap_or((recipe.size_w, recipe.size_h));
    let compiled = compile_recipe_for_size(project, recipe, size)?;
    let base_seed = seed_override.unwrap_or(recipe.seed);
    let params = Params::seeded(base_seed);

    for attempt in 0..params.max_attempts {
        let attempts = attempt + 1;
        let mut attempt_params = params;
        attempt_params.seed = base_seed.wrapping_add(attempt as u64);
        let slots = solve(
            compiled.grid,
            &compiled.rules,
            &compiled.presets,
            &attempt_params,
        )?;
        if let Some((symbol, limit)) = compiled.limits.iter().find(|(symbol, limit)| {
            let piece_id = compiled.symbols[*symbol as usize].piece_id;
            let count = slots
                .iter()
                .filter(|&&chosen| {
                    let info = &compiled.symbols[chosen as usize];
                    info.piece_id == piece_id && info.is_anchor
                })
                .count() as u32;
            count > *limit
        }) {
            return if attempts == params.max_attempts {
                Err(GenerateError::LimitUnsatisfied {
                    piece_id: compiled.symbols[*symbol as usize].piece_id,
                    limit: *limit,
                    attempts,
                })
            } else {
                continue;
            };
        }
        return bake(&compiled, &slots, attempt_params.seed, attempts);
    }

    // Params::max_attempts is currently positive in cgfx-wfc. Keep this guard
    // if that contract changes, rather than returning an uninitialized hunk.
    let (symbol, limit) = compiled.limits.first().copied().unwrap_or((0, 0));
    Err(GenerateError::LimitUnsatisfied {
        piece_id: compiled
            .symbols
            .get(symbol as usize)
            .map_or(0, |info| info.piece_id),
        limit,
        attempts: params.max_attempts,
    })
}
