// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::prelude::*;
use std::collections::HashMap;

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
pub struct Solution {
    options: api::OptionMap,
    resolved: HashMap<api::PkgRequest, (api::Spec, PackageSource)>,
    by_name: HashMap<String, api::Spec>,
}

impl Solution {
    pub fn new(options: Option<api::OptionMap>) -> Self {
        Self {
            options: options.unwrap_or_default(),
            resolved: HashMap::default(),
            by_name: HashMap::default(),
        }
    }

    pub fn add(&mut self, request: &api::PkgRequest, package: &api::Spec, source: &PackageSource) {
        self.resolved
            .insert(request.clone(), (package.clone(), source.clone()));
        self.by_name
            .insert(request.pkg.name().to_owned(), package.clone());
    }
}

#[pymethods]
impl Solution {
    pub fn options(&self) -> api::OptionMap {
        self.options.clone()
    }
}
