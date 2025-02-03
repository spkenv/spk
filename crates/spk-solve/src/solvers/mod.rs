// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Spk package solver implementations.

pub(crate) mod resolvo;
pub(crate) mod step;

pub use resolvo::Solver as ResolvoSolver;
pub use step::{ErrorFreq, Solver as StepSolver, SolverRuntime as StepSolverRuntime};

// Public to allow other tests to use its macros
#[cfg(test)]
#[path = "./solver_test.rs"]
mod solver_test;
