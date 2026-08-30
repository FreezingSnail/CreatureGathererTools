use std::error::Error;
use std::fmt;

use cgfx_wfc::{ObservationFilter, Params, WfcError, solve_with_filter};

use super::bake::{Hunk, bake};
use super::compile::compile_recipe_for_size;
use super::{RuleSpec, ValidationIssue, WfcProject};

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
    RuleUnsatisfied {
        rule: String,
        attempts: u32,
    },
    /// Backwards-compatible error for the legacy `WfcPiece::limit` shorthand.
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
            Self::RuleUnsatisfied { rule, attempts } => {
                write!(f, "rule {rule} unsatisfied after {attempts} attempts")
            }
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

struct RuleFilter<'a> {
    grid: cgfx_wfc::Grid,
    symbols: &'a [super::SymbolInfo],
    rules: &'a [RuleSpec],
}

impl RuleFilter<'_> {
    fn piece(&self, symbol: u32) -> Option<&super::SymbolInfo> {
        self.symbols.get(symbol as usize)
    }

    fn anchor_count(&self, decided: &[Option<u32>], piece_id: u32) -> u32 {
        decided
            .iter()
            .flatten()
            .filter_map(|symbol| self.piece(*symbol))
            .filter(|info| info.is_anchor && info.piece_id == piece_id)
            .count() as u32
    }

    fn near_piece(&self, cell: usize, decided: &[Option<u32>], piece_id: u32, radius: u32) -> bool {
        let (x, y) = self.grid.coords(cell);
        decided.iter().enumerate().any(|(other_cell, symbol)| {
            let Some(info) = symbol.and_then(|value| self.piece(value)) else {
                return false;
            };
            let (other_x, other_y) = self.grid.coords(other_cell);
            info.is_anchor
                && info.piece_id == piece_id
                && x.abs_diff(other_x).max(y.abs_diff(other_y)) <= radius
        })
    }

    fn rule_name(&self) -> String {
        self.rules
            .iter()
            .find(|rule| {
                matches!(
                    rule,
                    RuleSpec::Count { max: Some(_), .. } | RuleSpec::Exclude { .. }
                )
            })
            .map(format_rule)
            .unwrap_or_else(|| "observation filter".to_string())
    }
}

impl ObservationFilter for RuleFilter<'_> {
    fn allows(&self, cell: usize, symbol: u32, decided: &[Option<u32>]) -> bool {
        let Some(info) = self.piece(symbol) else {
            return false;
        };
        for rule in self.rules {
            match rule {
                RuleSpec::Count {
                    piece_id,
                    max: Some(max),
                    ..
                } if info.is_anchor && info.piece_id == *piece_id => {
                    if self.anchor_count(decided, *piece_id) >= *max {
                        return false;
                    }
                }
                RuleSpec::Exclude { a, b, radius }
                    if info.is_anchor
                        && ((info.piece_id == *a
                            && self.near_piece(cell, decided, *b, *radius))
                            || (info.piece_id == *b
                                && self.near_piece(cell, decided, *a, *radius))) =>
                {
                    return false;
                }
                _ => {}
            }
        }
        true
    }
}

fn format_rule(rule: &RuleSpec) -> String {
    match rule {
        RuleSpec::Count { piece_id, min, max } => {
            format!("count piece {piece_id} min {min:?} max {max:?}")
        }
        RuleSpec::Exclude { a, b, radius } => format!("exclude pieces {a}/{b} radius {radius}"),
        RuleSpec::Border { piece_ids } => format!("border pieces {piece_ids:?}"),
        RuleSpec::Band {
            piece_id,
            from_row,
            to_row,
        } => {
            format!("band piece {piece_id} rows {from_row}..={to_row}")
        }
    }
}

fn count_anchors(slots: &[u32], symbols: &[super::SymbolInfo], piece_id: u32) -> u32 {
    slots
        .iter()
        .filter_map(|symbol| symbols.get(*symbol as usize))
        .filter(|info| info.is_anchor && info.piece_id == piece_id)
        .count() as u32
}

fn rule_satisfied(
    rule: &RuleSpec,
    slots: &[u32],
    symbols: &[super::SymbolInfo],
    grid: cgfx_wfc::Grid,
) -> bool {
    match rule {
        RuleSpec::Count { piece_id, min, max } => {
            let count = count_anchors(slots, symbols, *piece_id);
            min.is_none_or(|value| count >= value) && max.is_none_or(|value| count <= value)
        }
        RuleSpec::Exclude { a, b, radius } => {
            for (cell, symbol) in slots.iter().enumerate() {
                let Some(left) = symbols.get(*symbol as usize) else {
                    return false;
                };
                if !left.is_anchor || (left.piece_id != *a && left.piece_id != *b) {
                    continue;
                }
                let (x, y) = grid.coords(cell);
                for (other_cell, other_symbol) in slots.iter().enumerate().skip(cell + 1) {
                    let Some(right) = symbols.get(*other_symbol as usize) else {
                        return false;
                    };
                    if !right.is_anchor
                        || x.abs_diff(grid.coords(other_cell).0)
                            .max(y.abs_diff(grid.coords(other_cell).1))
                            > *radius
                    {
                        continue;
                    }
                    if (left.piece_id == *a && right.piece_id == *b)
                        || (left.piece_id == *b && right.piece_id == *a)
                    {
                        return false;
                    }
                }
            }
            true
        }
        RuleSpec::Border { piece_ids } => slots.iter().enumerate().all(|(cell, symbol)| {
            let (x, y) = grid.coords(cell);
            let border = x == 0 || y == 0 || x + 1 == grid.w || y + 1 == grid.h;
            !border
                || symbols
                    .get(*symbol as usize)
                    .is_some_and(|info| piece_ids.contains(&info.piece_id))
        }),
        RuleSpec::Band {
            piece_id,
            from_row,
            to_row,
        } => slots.iter().enumerate().all(|(cell, symbol)| {
            let row = grid.coords(cell).1;
            row >= *from_row && row <= *to_row
                || symbols
                    .get(*symbol as usize)
                    .is_none_or(|info| info.piece_id != *piece_id)
        }),
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
    let filter = RuleFilter {
        grid: compiled.grid,
        symbols: &compiled.symbols,
        rules: &compiled.placement_rules,
    };

    for attempt in 0..params.max_attempts {
        let attempts = attempt + 1;
        let mut attempt_params = params;
        attempt_params.seed = base_seed.wrapping_add(attempt as u64);
        let slots = match solve_with_filter(
            compiled.grid,
            &compiled.rules,
            &compiled.presets,
            &attempt_params,
            &filter,
        ) {
            Ok(slots) => slots,
            Err(WfcError::ObservationContradiction { .. }) => {
                if attempts == params.max_attempts {
                    if let Some(RuleSpec::Count {
                        piece_id,
                        max: Some(limit),
                        ..
                    }) = compiled
                        .placement_rules
                        .iter()
                        .find(|rule| matches!(rule, RuleSpec::Count { max: Some(_), .. }))
                        && project
                            .pieces
                            .iter()
                            .find(|piece| piece.id == *piece_id)
                            .is_some_and(|piece| piece.limit == Some(*limit))
                    {
                        return Err(GenerateError::LimitUnsatisfied {
                            piece_id: *piece_id,
                            limit: *limit,
                            attempts,
                        });
                    }
                    return Err(GenerateError::RuleUnsatisfied {
                        rule: filter.rule_name(),
                        attempts,
                    });
                }
                continue;
            }
            Err(error) => return Err(GenerateError::Solver(error)),
        };

        if let Some(rule) = compiled
            .placement_rules
            .iter()
            .find(|rule| !rule_satisfied(rule, &slots, &compiled.symbols, compiled.grid))
        {
            if attempts == params.max_attempts {
                if let RuleSpec::Count {
                    piece_id,
                    max: Some(limit),
                    ..
                } = rule
                    && project
                        .pieces
                        .iter()
                        .find(|piece| piece.id == *piece_id)
                        .is_some_and(|piece| piece.limit == Some(*limit))
                {
                    return Err(GenerateError::LimitUnsatisfied {
                        piece_id: *piece_id,
                        limit: *limit,
                        attempts,
                    });
                }
                return Err(GenerateError::RuleUnsatisfied {
                    rule: format_rule(rule),
                    attempts,
                });
            }
            continue;
        }
        return bake(&compiled, &slots, attempt_params.seed, attempts);
    }

    Err(GenerateError::RuleUnsatisfied {
        rule: "placement rule".to_string(),
        attempts: params.max_attempts,
    })
}
