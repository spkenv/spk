// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::prelude::*;

/// Denotes that a test has failed or was invalid.
#[derive(Debug)]
pub struct TestError {
    pub message: String,
}

impl TestError {
    pub fn new_error(msg: String) -> crate::Error {
        crate::Error::Test(Self { message: msg })
    }
}

#[pyclass]
pub struct PackageBuildTester {}
