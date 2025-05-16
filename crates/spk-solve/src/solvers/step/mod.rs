// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Spk's original solver.
//!
//! This solver works by using a depth-first search to find a solution to the
//! list of requests. It is fastest when few or no conflicting requests are
//! encountered when picking candidates, but as the number of candidates grows
//! then its mostly brute force approach gets overwhelmed.

mod solver;

#[cfg(test)]
pub(crate) use solver::ErrorDetails;
pub use solver::{ErrorFreq, Solver, SolverRuntime};
