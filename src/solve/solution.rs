// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;

use crate::{
    api::{self, Ident},
    error,
};

#[derive(Clone)]
pub enum PackageSource {
    // todo!() Repository(Repository),
    Spec(api::Spec),
}

impl PackageSource {
    pub fn read_spec(&self, _ident: &Ident) -> error::Result<api::Spec> {
        todo!()
    }
}

/// Represents a set of resolved packages.
#[pyclass]
pub struct Solution {}
