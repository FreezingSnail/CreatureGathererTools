//! Adjacency rules over opaque symbols.
//!
//! A "symbol" is just an index. This crate never learns what one means — the
//! caller (`creaturevm`'s wfcmap adapter) maps symbols to authored tile groups.

use crate::Dir;

/// Which symbols may sit in each direction from each symbol, plus how often
/// each symbol should be chosen.
///
/// Stored as adjacency lists rather than a bitset matrix: the propagator walks
/// `allow[dir][symbol]` linearly on every ban, and authored rulesets are sparse
/// (a group typically mates with a handful of others per edge).
#[derive(Debug, Clone)]
pub struct Rules {
    n: usize,
    /// `allow[dir][a]` = symbols permitted in the neighbour that lies in
    /// direction `dir` from a cell holding `a`.
    allow: [Vec<Vec<u32>>; 4],
    weights: Vec<f64>,
}

/// Rejected rulesets, reported before solving so callers get a precise cause
/// instead of an unexplained contradiction mid-solve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RulesError {
    /// A ruleset with no symbols can never fill a non-empty grid.
    NoSymbols,
    SymbolOutOfRange {
        symbol: u32,
        n: usize,
    },
    NonPositiveWeight {
        symbol: u32,
    },
    /// `b` may sit north of `a` but `a` may not sit south of `b`. The
    /// propagator assumes symmetry; an asymmetric table would let it derive
    /// placements the caller never authorised.
    Asymmetric {
        a: u32,
        dir: Dir,
        b: u32,
    },
}

impl std::fmt::Display for RulesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RulesError::NoSymbols => write!(f, "ruleset has no symbols"),
            RulesError::SymbolOutOfRange { symbol, n } => {
                write!(f, "symbol {symbol} out of range for {n} symbols")
            }
            RulesError::NonPositiveWeight { symbol } => {
                write!(f, "symbol {symbol} has a non-positive weight")
            }
            RulesError::Asymmetric { a, dir, b } => write!(
                f,
                "asymmetric rule: {b} allowed {dir:?} of {a}, but {a} not allowed {:?} of {b}",
                dir.opposite()
            ),
        }
    }
}

impl std::error::Error for RulesError {}

impl Rules {
    /// All symbols start with weight 1 and no permitted neighbours.
    pub fn new(n: usize) -> Self {
        Rules {
            n,
            allow: [
                vec![Vec::new(); n],
                vec![Vec::new(); n],
                vec![Vec::new(); n],
                vec![Vec::new(); n],
            ],
            weights: vec![1.0; n],
        }
    }

    pub fn symbol_count(&self) -> usize {
        self.n
    }

    /// Records that `b` may sit in direction `dir` from `a`, and the mirrored
    /// fact that `a` may sit opposite `b`.
    ///
    /// This is the intended entry point. Adjacency is inherently mutual, and
    /// requiring callers to state both halves is a reliable source of rulesets
    /// that fail only deep into a solve.
    pub fn allow(&mut self, a: u32, dir: Dir, b: u32) -> &mut Self {
        push_unique(&mut self.allow[dir as usize], a, b);
        push_unique(&mut self.allow[dir.opposite() as usize], b, a);
        self
    }

    /// Records one half of an adjacency without its mirror.
    ///
    /// For rule tables built by other means (imported, generated) that will be
    /// checked with [`Rules::validate`] afterwards. Prefer [`Rules::allow`],
    /// which cannot leave the table asymmetric.
    pub fn allow_directed(&mut self, a: u32, dir: Dir, b: u32) -> &mut Self {
        push_unique(&mut self.allow[dir as usize], a, b);
        self
    }

    /// Allows `b` on every side of `a` (and vice versa).
    pub fn allow_all_dirs(&mut self, a: u32, b: u32) -> &mut Self {
        for dir in Dir::ALL {
            self.allow(a, dir, b);
        }
        self
    }

    /// Relative selection frequency. Higher weights are picked more often and
    /// also raise a cell's entropy, so heavily-weighted symbols are resolved
    /// later — matching Gumin's reference behaviour.
    pub fn set_weight(&mut self, symbol: u32, weight: f64) -> &mut Self {
        if let Some(slot) = self.weights.get_mut(symbol as usize) {
            *slot = weight;
        }
        self
    }

    pub fn weights(&self) -> &[f64] {
        &self.weights
    }

    pub(crate) fn allowed(&self, dir: Dir, symbol: u32) -> &[u32] {
        &self.allow[dir as usize][symbol as usize]
    }

    /// Number of neighbour symbols in `dir` that support `symbol`; the initial
    /// value of the propagator's support counter.
    pub(crate) fn support_count(&self, dir: Dir, symbol: u32) -> u32 {
        self.allow[dir as usize][symbol as usize].len() as u32
    }

    /// Checks the invariants the solver relies on. Called by `solve`, but
    /// exposed so authoring tools can validate a ruleset as it is edited.
    pub fn validate(&self) -> Result<(), RulesError> {
        if self.n == 0 {
            return Err(RulesError::NoSymbols);
        }
        for (i, &w) in self.weights.iter().enumerate() {
            if !w.is_finite() || w.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
                return Err(RulesError::NonPositiveWeight { symbol: i as u32 });
            }
        }
        for dir in Dir::ALL {
            for a in 0..self.n {
                for &b in &self.allow[dir as usize][a] {
                    if b as usize >= self.n {
                        return Err(RulesError::SymbolOutOfRange {
                            symbol: b,
                            n: self.n,
                        });
                    }
                    if !self.allow[dir.opposite() as usize][b as usize].contains(&(a as u32)) {
                        return Err(RulesError::Asymmetric {
                            a: a as u32,
                            dir,
                            b,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

fn push_unique(lists: &mut [Vec<u32>], key: u32, value: u32) {
    if let Some(list) = lists.get_mut(key as usize)
        && !list.contains(&value)
    {
        list.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_records_both_directions() {
        let mut r = Rules::new(2);
        r.allow(0, Dir::N, 1);
        assert_eq!(r.allowed(Dir::N, 0), &[1]);
        assert_eq!(r.allowed(Dir::S, 1), &[0]);
        assert!(r.validate().is_ok());
    }

    #[test]
    fn allow_is_idempotent() {
        let mut r = Rules::new(2);
        r.allow(0, Dir::E, 1);
        r.allow(0, Dir::E, 1);
        assert_eq!(r.allowed(Dir::E, 0), &[1]);
    }

    #[test]
    fn support_count_counts_opposite_side() {
        let mut r = Rules::new(3);
        r.allow(0, Dir::N, 2);
        r.allow(1, Dir::N, 2);
        // Two symbols (0 and 1) can sit south of 2, so 2 has two supporters
        // when we ask what may lie to its south.
        assert_eq!(r.support_count(Dir::S, 2), 2);
    }

    #[test]
    fn validate_rejects_empty_and_bad_weights() {
        assert_eq!(Rules::new(0).validate(), Err(RulesError::NoSymbols));
        let mut r = Rules::new(1);
        r.allow_all_dirs(0, 0);
        r.set_weight(0, 0.0);
        assert_eq!(
            r.validate(),
            Err(RulesError::NonPositiveWeight { symbol: 0 })
        );
    }
}
