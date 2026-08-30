use cgfx_wfc::{Dir, Grid, Params, Preset, Rules, WfcError, solve};

fn checkerboard_rules() -> Rules {
    let mut rules = Rules::new(2);
    rules.allow_all_dirs(0, 1);
    rules
}

fn assert_checkerboard(grid: Grid, output: &[u32]) {
    assert_eq!(output.len(), grid.cells());
    for cell in 0..grid.cells() {
        for dir in Dir::ALL {
            if let Some(neighbour) = grid.neighbour(cell, dir) {
                assert_ne!(output[cell], output[neighbour]);
            }
        }
    }
}

#[test]
fn same_seed_is_deterministic_and_output_satisfies_adjacency() {
    let grid = Grid::new(8, 6);
    let rules = checkerboard_rules();
    let params = Params::seeded(91);
    let first = solve(grid, &rules, &[], &params).unwrap();
    let second = solve(grid, &rules, &[], &params).unwrap();
    assert_eq!(first, second);
    assert_checkerboard(grid, &first);
}

#[test]
fn checkerboard_has_different_four_neighbours() {
    let grid = Grid::new(4, 4);
    let output = solve(grid, &checkerboard_rules(), &[], &Params::seeded(1)).unwrap();
    for cell in 0..grid.cells() {
        for dir in Dir::ALL {
            if let Some(neighbour) = grid.neighbour(cell, dir) {
                assert_ne!(output[cell], output[neighbour]);
            }
        }
    }
}

#[test]
fn single_symbol_fills_grid() {
    let mut rules = Rules::new(1);
    rules.allow_all_dirs(0, 0);
    let output = solve(Grid::new(7, 5), &rules, &[], &Params::seeded(3)).unwrap();
    assert_eq!(output, vec![0; 35]);
}

#[test]
fn presets_are_respected_and_propagate() {
    let grid = Grid::new(4, 1);
    let rules = checkerboard_rules();
    let output = solve(
        grid,
        &rules,
        &[Preset {
            cell: 0,
            allowed: vec![1],
        }],
        &Params::seeded(8),
    )
    .unwrap();
    assert_eq!(output[0], 1);
    assert_eq!(output[1], 0);
    assert_checkerboard(grid, &output);
}

#[test]
fn incompatible_presets_return_contradiction() {
    let rules = checkerboard_rules();
    let error = solve(
        Grid::new(2, 1),
        &rules,
        &[
            Preset {
                cell: 0,
                allowed: vec![0],
            },
            Preset {
                cell: 1,
                allowed: vec![0],
            },
        ],
        &Params::seeded(2),
    )
    .unwrap_err();
    assert!(matches!(error, WfcError::Contradiction { .. }));
}

#[test]
fn step_budget_is_not_retried() {
    let error = solve(
        Grid::new(3, 3),
        &checkerboard_rules(),
        &[],
        &Params {
            budget_steps: 1,
            ..Params::seeded(4)
        },
    )
    .unwrap_err();
    assert_eq!(error, WfcError::BudgetExceeded { steps: 2 });
}

#[test]
fn memory_budget_reports_required_bytes() {
    let error = solve(
        Grid::new(2, 2),
        &checkerboard_rules(),
        &[],
        &Params {
            max_bytes: 1,
            ..Params::seeded(4)
        },
    )
    .unwrap_err();
    assert_eq!(
        error,
        WfcError::MemoryBudget {
            needed: 4 * 2 * 4 * 2,
            cap: 1,
        }
    );
}

#[test]
fn asymmetric_rules_are_rejected() {
    let mut rules = Rules::new(2);
    rules.allow_directed(0, Dir::E, 1);
    let error = solve(Grid::new(2, 1), &rules, &[], &Params::seeded(0)).unwrap_err();
    assert!(matches!(
        error,
        WfcError::Rules(cgfx_wfc::RulesError::Asymmetric { .. })
    ));
}

#[test]
fn periodic_edges_satisfy_adjacency() {
    let grid = Grid::periodic(4, 4);
    let rules = checkerboard_rules();
    let output = solve(grid, &rules, &[], &Params::seeded(14)).unwrap();
    assert_checkerboard(grid, &output);
}

#[test]
fn golden_seed_7() {
    let mut rules = Rules::new(3);
    for a in 0..3 {
        for b in 0..3 {
            rules.allow_all_dirs(a, b);
        }
    }
    rules
        .set_weight(0, 1.0)
        .set_weight(1, 2.0)
        .set_weight(2, 3.0);
    let output = solve(Grid::new(8, 8), &rules, &[], &Params::seeded(7)).unwrap();
    assert_eq!(
        output,
        vec![
            2, 2, 2, 1, 0, 2, 0, 0, 2, 2, 2, 2, 2, 1, 2, 2, 1, 2, 2, 1, 2, 1, 2, 2, 1, 2, 1, 2, 2,
            2, 2, 2, 2, 2, 1, 2, 1, 0, 2, 1, 2, 2, 1, 1, 0, 1, 2, 2, 1, 2, 1, 2, 2, 2, 0, 2, 1, 2,
            1, 1, 1, 2, 0, 2,
        ]
    );
}
