// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "./component_spec_test.rs"]
mod component_spec_test;

/// Defines a named package component.
#[pyclass]
#[derive(Debug, Default, Hash, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentSpec {}
