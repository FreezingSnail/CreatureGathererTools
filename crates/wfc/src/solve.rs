use crate::{Dir, Grid, Rng, Rules, RulesError};

/// Limits and seed controlling one deterministic solve.
#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub seed: u64,
    pub max_attempts: u32,
    pub budget_steps: u64,
    pub max_bytes: usize,
}

impl Params {
    pub fn seeded(seed: u64) -> Self {
        Self {
            seed,
            max_attempts: 10,
            budget_steps: 5_000_000,
            max_bytes: 128 << 20,
        }
    }
}

/// A cell whose domain must be restricted to the listed symbols.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    pub cell: usize,
    pub allowed: Vec<u32>,
}

/// Domain-free observation policy. `decided` contains the symbols fixed by
/// earlier observations, in row-major cell order; undecided cells are `None`.
pub trait ObservationFilter {
    fn allows(&self, cell: usize, symbol: u32, decided: &[Option<u32>]) -> bool;
}

struct AllowAll;

impl ObservationFilter for AllowAll {
    fn allows(&self, _cell: usize, _symbol: u32, _decided: &[Option<u32>]) -> bool {
        true
    }
}

/// Failure while validating or solving a wave.
#[derive(Debug, Clone, PartialEq)]
pub enum WfcError {
    Rules(RulesError),
    EmptyGrid,
    PresetOutOfRange {
        cell: usize,
        cells: usize,
    },
    PresetEmpty {
        cell: usize,
    },
    MemoryBudget {
        needed: usize,
        cap: usize,
    },
    Contradiction {
        cell: usize,
        x: u32,
        y: u32,
        attempts: u32,
    },
    BudgetExceeded {
        steps: u64,
    },
    ObservationContradiction {
        cell: usize,
        x: u32,
        y: u32,
    },
}

impl std::fmt::Display for WfcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rules(error) => write!(f, "invalid rules: {error}"),
            Self::EmptyGrid => write!(f, "grid has no cells"),
            Self::PresetOutOfRange { cell, cells } => {
                write!(f, "preset cell {cell} out of range for {cells} cells")
            }
            Self::PresetEmpty { cell } => write!(f, "preset for cell {cell} is empty"),
            Self::MemoryBudget { needed, cap } => {
                write!(f, "memory budget exceeded: need {needed} bytes, cap {cap}")
            }
            Self::Contradiction {
                cell,
                x,
                y,
                attempts,
            } => write!(
                f,
                "contradiction at cell {cell} ({x}, {y}) after {attempts} attempts"
            ),
            Self::BudgetExceeded { steps } => write!(f, "step budget exceeded at step {steps}"),
            Self::ObservationContradiction { cell, x, y } => {
                write!(f, "observation filter rejected cell {cell} ({x}, {y})")
            }
        }
    }
}

impl std::error::Error for WfcError {}

impl From<RulesError> for WfcError {
    fn from(error: RulesError) -> Self {
        Self::Rules(error)
    }
}

/// Solve a rectangular wave using deterministic observation and restart-only
/// recovery. Symbols are opaque indices; all domain meaning belongs to callers.
pub fn solve(
    grid: Grid,
    rules: &Rules,
    presets: &[Preset],
    params: &Params,
) -> Result<Vec<u32>, WfcError> {
    solve_with_filter(grid, rules, presets, params, &AllowAll)
}

/// Solve with an optional domain-free policy applied at each observation.
pub fn solve_with_filter<F: ObservationFilter + ?Sized>(
    grid: Grid,
    rules: &Rules,
    presets: &[Preset],
    params: &Params,
    filter: &F,
) -> Result<Vec<u32>, WfcError> {
    rules.validate()?;
    let cells = grid.cells();
    if cells == 0 {
        return Err(WfcError::EmptyGrid);
    }
    let n = rules.symbol_count();
    for preset in presets {
        if preset.cell >= cells {
            return Err(WfcError::PresetOutOfRange {
                cell: preset.cell,
                cells,
            });
        }
        if preset.allowed.is_empty() {
            return Err(WfcError::PresetEmpty { cell: preset.cell });
        }
        for &symbol in &preset.allowed {
            if symbol as usize >= n {
                return Err(WfcError::Rules(RulesError::SymbolOutOfRange { symbol, n }));
            }
        }
    }

    let Some(needed) = cells
        .checked_mul(n)
        .and_then(|entries| entries.checked_mul(4))
        .and_then(|entries| entries.checked_mul(std::mem::size_of::<u16>()))
    else {
        return Err(WfcError::MemoryBudget {
            needed: usize::MAX,
            cap: params.max_bytes,
        });
    };
    if needed > params.max_bytes {
        return Err(WfcError::MemoryBudget {
            needed,
            cap: params.max_bytes,
        });
    }

    // A zero-attempt parameter cannot produce a useful result; treating it as
    // one attempt preserves the typed contradiction result and avoids a panic.
    let attempts = params.max_attempts.max(1);
    let mut last_contradiction = None;
    let mut last_filter_contradiction = None;
    for attempt in 0..attempts {
        let mut state = State::new(grid, rules);
        let mut rng = Rng::new(params.seed.wrapping_add(attempt as u64));
        let result = state.initialize(rules, presets, grid, params.budget_steps, &mut rng);
        match result {
            Ok(()) => match state.observe(rules, grid, params.budget_steps, &mut rng, filter) {
                Ok(output) => return Ok(output),
                Err(WfcError::ObservationContradiction { cell, .. }) => {
                    last_filter_contradiction = Some(cell);
                }
                Err(WfcError::Contradiction { cell, .. }) => {
                    last_contradiction = Some(cell);
                }
                Err(error) => return Err(error),
            },
            Err(WfcError::Contradiction { cell, .. }) => {
                last_contradiction = Some(cell);
            }
            Err(error) => return Err(error),
        }
    }

    if let Some(cell) = last_filter_contradiction {
        let (x, y) = grid.coords(cell);
        return Err(WfcError::ObservationContradiction { cell, x, y });
    }
    let cell = last_contradiction.map_or(0, |cell| cell);
    let (x, y) = grid.coords(cell);
    Err(WfcError::Contradiction {
        cell,
        x,
        y,
        attempts,
    })
}

struct State {
    wave: Vec<bool>,
    support: Vec<u16>,
    sums_of_ones: Vec<u32>,
    sums_of_weights: Vec<f64>,
    sums_of_weight_log_weights: Vec<f64>,
    entropies: Vec<f64>,
    stack: Vec<(usize, u32)>,
    steps: u64,
}

impl State {
    fn new(grid: Grid, rules: &Rules) -> Self {
        let cells = grid.cells();
        let n = rules.symbol_count();
        let entries = cells * n;
        let mut state = Self {
            wave: vec![true; entries],
            support: vec![0; entries * 4],
            sums_of_ones: vec![n as u32; cells],
            sums_of_weights: Vec::with_capacity(cells),
            sums_of_weight_log_weights: Vec::with_capacity(cells),
            entropies: Vec::with_capacity(cells),
            stack: Vec::new(),
            steps: 0,
        };
        let (sum_weights, sum_weight_log_weights) = rules
            .weights()
            .iter()
            .fold((0.0, 0.0), |(sum, log_sum), &weight| {
                (sum + weight, log_sum + weight * weight.ln())
            });
        let entropy = entropy(sum_weights, sum_weight_log_weights);
        state.sums_of_weights.resize(cells, sum_weights);
        state
            .sums_of_weight_log_weights
            .resize(cells, sum_weight_log_weights);
        state.entropies.resize(cells, entropy);
        for cell in 0..cells {
            for symbol in 0..n {
                for dir in Dir::ALL {
                    let support = rules.support_count(dir, symbol as u32);
                    state.support[(cell * n + symbol) * 4 + dir as usize] =
                        support.min(u16::MAX as u32) as u16;
                    if grid.neighbour(cell, dir).is_some() && support == 0 {
                        state.ban(cell, symbol as u32, rules);
                    }
                }
            }
        }
        state
    }

    fn initialize(
        &mut self,
        rules: &Rules,
        presets: &[Preset],
        grid: Grid,
        budget: u64,
        rng: &mut Rng,
    ) -> Result<(), WfcError> {
        for preset in presets {
            for symbol in 0..rules.symbol_count() as u32 {
                if !preset.allowed.contains(&symbol) {
                    self.ban(preset.cell, symbol, rules);
                }
            }
        }
        self.propagate(rules, grid, budget, rng)
    }

    fn ban(&mut self, cell: usize, symbol: u32, rules: &Rules) {
        let n = rules.symbol_count();
        let index = cell * n + symbol as usize;
        if !self.wave[index] {
            return;
        }
        self.wave[index] = false;
        for dir in Dir::ALL {
            self.support[index * 4 + dir as usize] = 0;
        }
        self.stack.push((cell, symbol));
        self.sums_of_ones[cell] -= 1;
        let weight = rules.weights()[symbol as usize];
        self.sums_of_weights[cell] -= weight;
        self.sums_of_weight_log_weights[cell] -= weight * weight.ln();
        self.entropies[cell] = entropy(
            self.sums_of_weights[cell],
            self.sums_of_weight_log_weights[cell],
        );
    }

    fn propagate(
        &mut self,
        rules: &Rules,
        grid: Grid,
        budget: u64,
        _rng: &mut Rng,
    ) -> Result<(), WfcError> {
        let n = rules.symbol_count();
        while let Some((cell, symbol)) = self.stack.pop() {
            if self.sums_of_ones[cell] == 0 {
                return Err(self.contradiction(cell, grid, 0));
            }
            if self.steps >= budget {
                let steps = self.steps.saturating_add(1);
                return Err(WfcError::BudgetExceeded { steps });
            }
            self.steps += 1;
            for dir in Dir::ALL {
                let Some(neighbour) = grid.neighbour(cell, dir) else {
                    continue;
                };
                for &other in rules.allowed(dir, symbol) {
                    let support_index =
                        (neighbour * n + other as usize) * 4 + dir.opposite() as usize;
                    if !self.wave[neighbour * n + other as usize]
                        || self.support[support_index] == 0
                    {
                        continue;
                    }
                    self.support[support_index] -= 1;
                    if self.support[support_index] == 0 {
                        self.ban(neighbour, other, rules);
                        if self.sums_of_ones[neighbour] == 0 {
                            return Err(self.contradiction(neighbour, grid, 0));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn observe<F: ObservationFilter + ?Sized>(
        &mut self,
        rules: &Rules,
        grid: Grid,
        budget: u64,
        rng: &mut Rng,
        filter: &F,
    ) -> Result<Vec<u32>, WfcError> {
        loop {
            let decided: Vec<Option<u32>> = (0..grid.cells())
                .map(|candidate_cell| {
                    if self.sums_of_ones[candidate_cell] != 1 {
                        return None;
                    }
                    (0..rules.symbol_count() as u32).find(|&symbol| {
                        self.wave[candidate_cell * rules.symbol_count() + symbol as usize]
                    })
                })
                .collect();
            let mut best = None;
            let mut filter_changed = false;
            for candidate_cell in 0..grid.cells() {
                if self.sums_of_ones[candidate_cell] <= 1 {
                    continue;
                }
                for symbol in 0..rules.symbol_count() as u32 {
                    if self.wave[candidate_cell * rules.symbol_count() + symbol as usize]
                        && !filter.allows(candidate_cell, symbol, &decided)
                    {
                        self.ban(candidate_cell, symbol, rules);
                        filter_changed = true;
                    }
                }
            }
            if let Err(error) = self.propagate(rules, grid, budget, rng) {
                if filter_changed && let WfcError::Contradiction { cell, .. } = error {
                    let (x, y) = grid.coords(cell);
                    return Err(WfcError::ObservationContradiction { cell, x, y });
                }
                return Err(error);
            }
            for cell in 0..grid.cells() {
                if self.sums_of_ones[cell] > 1 {
                    let key = self.entropies[cell] + 1e-6 * rng.next_f64();
                    if best.is_none_or(|(_, best_key)| key < best_key) {
                        best = Some((cell, key));
                    }
                }
            }
            let Some((cell, _)) = best else {
                return Ok(self.output(rules));
            };

            let allowed: Vec<(u32, f64)> = (0..rules.symbol_count() as u32)
                .filter(|&symbol| {
                    self.wave[cell * rules.symbol_count() + symbol as usize]
                        && filter.allows(cell, symbol, &decided)
                })
                .map(|symbol| (symbol, rules.weights()[symbol as usize]))
                .collect();
            let allowed_weight: f64 = allowed.iter().map(|(_, weight)| weight).sum();
            if allowed.is_empty() || allowed_weight <= 0.0 {
                let (x, y) = grid.coords(cell);
                return Err(WfcError::ObservationContradiction { cell, x, y });
            }
            let mut target = rng.next_f64() * allowed_weight;
            let mut chosen = None;
            for (symbol, weight) in allowed {
                if target < weight {
                    chosen = Some(symbol);
                    break;
                }
                target -= weight;
            }
            let Some(chosen) = chosen else {
                // Positive finite weights and a non-empty domain make this
                // unreachable; retain a typed error if arithmetic changes.
                return Err(self.contradiction(cell, grid, 0));
            };
            for symbol in 0..rules.symbol_count() as u32 {
                if symbol != chosen {
                    self.ban(cell, symbol, rules);
                }
            }
            self.propagate(rules, grid, budget, rng)?;
        }
    }

    fn output(&self, rules: &Rules) -> Vec<u32> {
        let n = rules.symbol_count();
        self.sums_of_ones
            .iter()
            .enumerate()
            .map(|(cell, _)| {
                for symbol in 0..n {
                    if self.wave[cell * n + symbol] {
                        return symbol as u32;
                    }
                }
                0
            })
            .collect()
    }

    fn contradiction(&self, cell: usize, grid: Grid, attempts: u32) -> WfcError {
        let (x, y) = grid.coords(cell);
        WfcError::Contradiction {
            cell,
            x,
            y,
            attempts,
        }
    }
}

fn entropy(sum_weights: f64, sum_weight_log_weights: f64) -> f64 {
    if sum_weights > 0.0 {
        sum_weights.ln() - sum_weight_log_weights / sum_weights
    } else {
        0.0
    }
}
