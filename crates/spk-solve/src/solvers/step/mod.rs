// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod solver;

#[cfg(test)]
pub(crate) use solver::ErrorDetails;
pub use solver::{ErrorFreq, Solver, SolverRuntime};
