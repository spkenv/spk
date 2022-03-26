// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::exceptions::PyRuntimeError;

pyo3::create_exception!(spk, NoEnvironmentError, PyRuntimeError);
