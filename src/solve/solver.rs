// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use crate::api::OptionMap;

use super::solution::Solution;

#[pyclass]
pub struct Solver {}

#[pymethods]
impl Solver {
    #[new]
    fn new() -> Self {
        Solver {}
    }

    pub fn reset(&mut self) {
        todo!()
    }

    /// If true, only solve pre-built binary packages.
    ///
    /// When false, the solver may return packages where the build is not set.
    /// These packages are known to have a source package available, and the requested
    /// options are valid for a new build of that source package.
    /// These packages are not actually built as part of the solver process but their
    /// build environments are fully resolved and dependencies included
    pub fn set_binary_only(&mut self, _binary_only: bool) {
        todo!()
    }

    pub fn solve(&self) -> Solution {
        todo!()
    }

    pub fn update_options(&mut self, _options: OptionMap) {
        todo!()
    }
}
