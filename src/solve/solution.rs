// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{prelude::*, PyIterProtocol};
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

/// Represents a package request that has been resolved.
#[pyclass]
pub struct SolvedRequest {
    pub request: api::PkgRequest,
    pub spec: api::Spec,
    pub source: PackageSource,
}

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

#[pyclass]
pub struct SolvedRequestIter {
    iter: std::collections::hash_map::IntoIter<api::PkgRequest, (api::Spec, PackageSource)>,
}

#[pyproto]
impl PyIterProtocol for SolvedRequestIter {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<SolvedRequest> {
        slf.iter
            .next()
            .map(|(request, (spec, source))| SolvedRequest {
                request,
                spec,
                source,
            })
    }
}

#[pymethods]
impl Solution {
    pub fn items(&self) -> SolvedRequestIter {
        SolvedRequestIter {
            iter: self.resolved.clone().into_iter(),
        }
    }

    pub fn options(&self) -> api::OptionMap {
        self.options.clone()
    }
}
