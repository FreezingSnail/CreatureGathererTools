//! Wave function collapse over a rectangular grid of slots.
//!
//! Domain-free by design: this crate knows grid dimensions, opaque symbol
//! indices, adjacency rules and weights. It has no notion of tiles, maps or
//! files, so it can be tested exhaustively on synthetic rulesets and reused
//! outside the CreatureGatherer toolchain.
//!
//! ```
//! use cgfx_wfc::{Dir, Grid, Params, Rules, solve};
//!
//! // Two symbols that may only neighbour each other: a checkerboard.
//! let mut rules = Rules::new(2);
//! rules.allow_all_dirs(0, 1);
//!
//! let grid = Grid::new(4, 4);
//! let out = solve(grid, &rules, &[], &Params::seeded(1)).unwrap();
//! assert_ne!(out[0], out[1]);
//! ```

mod rng;
mod rules;
mod solve;

pub use rng::Rng;
pub use rules::{Rules, RulesError};
pub use solve::{Params, Preset, WfcError, solve};

/// The four von Neumann neighbours. Discriminants are stable: they index the
/// propagator's per-direction tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum Dir {
    N = 0,
    E = 1,
    S = 2,
    W = 3,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::N, Dir::E, Dir::S, Dir::W];

    pub fn opposite(self) -> Dir {
        match self {
            Dir::N => Dir::S,
            Dir::E => Dir::W,
            Dir::S => Dir::N,
            Dir::W => Dir::E,
        }
    }

    /// Grid-space delta, with y increasing downward to match the map editor's
    /// row-major tile grids.
    pub fn delta(self) -> (i64, i64) {
        match self {
            Dir::N => (0, -1),
            Dir::E => (1, 0),
            Dir::S => (0, 1),
            Dir::W => (-1, 0),
        }
    }
}

/// The slot grid being solved. Cells are addressed row-major.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub w: u32,
    pub h: u32,
    /// When set, edges wrap. Off for map-region generation, where the border is
    /// constrained by the surrounding hand-authored map instead.
    pub periodic: bool,
}

impl Grid {
    pub fn new(w: u32, h: u32) -> Self {
        Grid {
            w,
            h,
            periodic: false,
        }
    }

    pub fn periodic(w: u32, h: u32) -> Self {
        Grid {
            w,
            h,
            periodic: true,
        }
    }

    pub fn cells(&self) -> usize {
        self.w as usize * self.h as usize
    }

    pub fn index(&self, x: u32, y: u32) -> usize {
        y as usize * self.w as usize + x as usize
    }

    pub fn coords(&self, cell: usize) -> (u32, u32) {
        let w = self.w as usize;
        ((cell % w) as u32, (cell / w) as u32)
    }

    /// Neighbour of `cell` in `dir`, or `None` when it falls outside a
    /// non-periodic grid.
    pub fn neighbour(&self, cell: usize, dir: Dir) -> Option<usize> {
        let (x, y) = self.coords(cell);
        let (dx, dy) = dir.delta();
        let (mut nx, mut ny) = (x as i64 + dx, y as i64 + dy);
        if self.periodic {
            nx = nx.rem_euclid(self.w as i64);
            ny = ny.rem_euclid(self.h as i64);
        } else if nx < 0 || ny < 0 || nx >= self.w as i64 || ny >= self.h as i64 {
            return None;
        }
        Some(self.index(nx as u32, ny as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opposite_round_trips() {
        for dir in Dir::ALL {
            assert_eq!(dir.opposite().opposite(), dir);
        }
    }

    #[test]
    fn non_periodic_edges_have_no_neighbour() {
        let g = Grid::new(3, 3);
        assert_eq!(g.neighbour(0, Dir::N), None);
        assert_eq!(g.neighbour(0, Dir::W), None);
        assert_eq!(g.neighbour(0, Dir::E), Some(1));
        assert_eq!(g.neighbour(0, Dir::S), Some(3));
        assert_eq!(g.neighbour(8, Dir::S), None);
    }

    #[test]
    fn periodic_edges_wrap() {
        let g = Grid::periodic(3, 3);
        assert_eq!(g.neighbour(0, Dir::N), Some(6));
        assert_eq!(g.neighbour(0, Dir::W), Some(2));
    }

    #[test]
    fn coords_and_index_round_trip() {
        let g = Grid::new(8, 4);
        for cell in 0..g.cells() {
            let (x, y) = g.coords(cell);
            assert_eq!(g.index(x, y), cell);
        }
    }
}
