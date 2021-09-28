// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;

use super::Repository;
use crate::{api, Result};

#[derive(Default, Clone, Debug)]
pub struct MemRepository {
    specs: HashMap<String, HashMap<api::Version, api::Spec>>,
    packages: HashMap<
        String,
        HashMap<api::Version, HashMap<api::Build, (api::Spec, spfs::encoding::Digest)>>,
    >,
}

impl Repository for MemRepository {
    fn list_packages(&self) -> Result<Vec<String>> {
        Ok(self.specs.keys().map(|s| s.to_owned()).collect())
    }

    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        if let Some(specs) = self.specs.get(name) {
            Ok(specs.keys().map(|v| v.to_owned()).collect())
        } else {
            Ok(Vec::new())
        }
    }

    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        if let Some(versions) = self.packages.get(pkg.name()) {
            if let Some(builds) = versions.get(&pkg.version) {
                Ok(builds
                    .keys()
                    .map(|b| pkg.with_build(Some(b.clone())))
                    .collect())
            } else {
                Ok(Vec::new())
            }
        } else {
            Ok(Vec::new())
        }
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        // try:
        //     if not pkg.build:
        //         return self._specs[pkg.name][str(pkg.version)].copy()
        //     else:
        //         return self._packages[pkg.name][str(pkg.version)][pkg.build][0].copy()
        // except KeyError:
        //     raise PackageNotFoundError(pkg)
        todo!()
    }

    fn get_package(&self, pkg: &api::Ident) -> Result<spfs::encoding::Digest> {
        // if pkg.build is None:
        //     raise PackageNotFoundError(pkg)
        // try:
        //     return self._packages[pkg.name][str(pkg.version)][pkg.build][1]
        // except KeyError:
        //     raise PackageNotFoundError(pkg)
        todo!()
    }

    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        // try:
        //     del self._specs[spec.pkg.name][str(spec.pkg.version)]
        // except KeyError:
        //     pass
        // self.publish_spec(spec)
        todo!()
    }

    fn publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        // assert (
        //     spec.pkg.build is None
        // ), f"Spec must be published with no build, got {spec.pkg}"
        // assert (
        //     spec.pkg.build is None or not spec.pkg.build == api.EMDEDDED
        // ), "Cannot publish embedded package"
        // self._specs.setdefault(spec.pkg.name, {})
        // versions = self._specs[spec.pkg.name]
        // version = str(spec.pkg.version)
        // if version in versions:
        //     raise VersionExistsError(version)
        // versions[version] = spec.copy()
        todo!()
    }

    fn remove_spec(&mut self, pkg: &api::Ident) -> Result<()> {
        // try:
        //     del self._specs[pkg.name][str(pkg.version)]
        // except KeyError:
        //     raise PackageNotFoundError(pkg)
        todo!()
    }

    fn publish_package(&mut self, spec: api::Spec, digest: spfs::encoding::Digest) -> Result<()> {
        // if spec.pkg.build is None:
        //     raise ValueError(
        //         "Package must include a build in order to be published: "
        //         + str(spec.pkg)
        //     )

        // self._packages.setdefault(spec.pkg.name, {})
        // version = str(spec.pkg.version)
        // self._packages[spec.pkg.name].setdefault(version, {})
        // build = spec.pkg.build
        // self._packages[spec.pkg.name][version][build] = (spec.copy(), digest)
        todo!()
    }

    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()> {
        // if pkg.build is None:
        //     raise ValueError(
        //         "Package must include a build in order to be removed: " + str(pkg)
        //     )
        // try:
        //     del self._packages[pkg.name][str(pkg.version)][pkg.build]
        // except KeyError:
        //     raise PackageNotFoundError(pkg)
        todo!()
    }
}
