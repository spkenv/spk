// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

/// A single change made to a state.
#[pyclass(subclass)]
pub struct Change {}

/// The decision represents a choice made by the solver.
///
/// Each decision connects one state to another in the graph.
#[pyclass]
pub struct Decision {}

#[pyclass]
pub struct Graph {}

#[pyclass]
pub struct Node {}

#[pyclass(subclass)]
pub struct Note {}

#[pyclass(extends=Change)]
pub struct RequestPackage {}

#[pyclass(extends=Change)]
pub struct RequestVar {}

#[pyclass(extends=Change, subclass)]
pub struct SetOptions {}

#[pyclass(extends=Change, subclass)]
pub struct SetPackage {}

#[pyclass(extends=SetPackage)]
pub struct SetPackageBuild {}

pub struct State {}

#[pyclass(extends=Note)]
pub struct SkipPackageNote {}

#[pyclass(extends=Change)]
pub struct StepBack {}
