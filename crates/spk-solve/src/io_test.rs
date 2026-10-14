// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::{DecisionFormatterBuilder, MultiSolverKind};

/// Config parsing recognizes every single-solver choice for run and output.
#[rstest]
#[case("cli", MultiSolverKind::Unchanged)]
#[case("checks", MultiSolverKind::AllImpossibleChecks)]
#[case("resolvo", MultiSolverKind::Resolvo)]
#[case("Resolvo", MultiSolverKind::Resolvo)]
fn test_formatter_solver_selection_from_config(
    #[case] value: &str,
    #[case] expected: MultiSolverKind,
) {
    let config = spk_config::Solver {
        solver_to_run: value.to_string(),
        solver_to_show: value.to_string(),
        ..Default::default()
    };
    let builder = DecisionFormatterBuilder::from_config(&config);
    assert_eq!(builder.solver_to_run, expected);
    assert_eq!(builder.solver_to_show, expected);
}
