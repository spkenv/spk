// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use crate::api;
use pyo3::prelude::*;

use super::solution::PackageSource;

pub enum BuildIterator {
    EmptyBuildIterator,
    SortedBuildIterator(SortedBuildIterator),
}

impl Iterator for BuildIterator {
    type Item = (api::Spec, PackageSource);

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

#[derive(Clone)]
pub enum PackageIterator {
    RepositoryPackageIterator(RepositoryPackageIterator),
}

impl Iterator for &PackageIterator {
    type Item = (api::Ident, BuildIterator);

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl PackageIterator {
    /// Replaces the internal build iterator for version with the given one.
    pub fn set_builds(&self, _version: &api::Version, _builds: &BuildIterator) {
        todo!()
    }
}

/// A stateful cursor yielding package builds from a set of repositories.
#[derive(Clone)]
pub struct RepositoryPackageIterator {
    pub package_name: String,
    pub repos: Vec<PyObject>,
}

pub struct SortedBuildIterator {
    _options: api::OptionMap,
    // Use Box to break recursive definition; lifetimes
    // can't be used because PyO3.
    _source: Box<BuildIterator>,
}

impl Iterator for SortedBuildIterator {
    type Item = BuildIterator;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}

impl SortedBuildIterator {
    pub fn new(_options: &api::OptionMap, _source: &BuildIterator) -> Self {
        todo!()
    }
}
