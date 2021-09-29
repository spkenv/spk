// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;

use super::Repository;
use crate::{api, Error, Result};

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
        match &pkg.build {
            None => self
                .specs
                .get(pkg.name())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .map(|s| s.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
            Some(build) => self
                .packages
                .get(pkg.name())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(build)
                .map(|(b, _)| b.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
        }
    }

    fn get_package(&self, pkg: &api::Ident) -> Result<spfs::encoding::Digest> {
        match &pkg.build {
            None => Err(Error::PackageNotFoundError(pkg.clone())),
            Some(build) => self
                .packages
                .get(pkg.name())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(&pkg.version)
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone()))?
                .get(build)
                .map(|(_, d)| d.to_owned())
                .ok_or_else(|| Error::PackageNotFoundError(pkg.clone())),
        }
    }

    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        let versions = match self.specs.get_mut(spec.pkg.name()) {
            Some(v) => v,
            None => {
                self.specs
                    .insert(spec.pkg.name().to_string(), Default::default());
                self.specs.get_mut(spec.pkg.name()).unwrap()
            }
        };
        versions.remove(&spec.pkg.version);
        self.publish_spec(spec)
    }

    fn publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        if spec.pkg.build.is_some() {
            return Err(Error::String(format!(
                "Spec must be published with no build, got {}",
                spec.pkg
            )));
        }
        let versions = match self.specs.get_mut(spec.pkg.name()) {
            Some(v) => v,
            None => {
                self.specs
                    .insert(spec.pkg.name().to_string(), Default::default());
                self.specs.get_mut(spec.pkg.name()).unwrap()
            }
        };
        if versions.contains_key(&spec.pkg.version) {
            Err(Error::VersionExistsError(spec.pkg.clone()))
        } else {
            versions.insert(spec.pkg.version.clone(), spec);
            Ok(())
        }
    }

    fn remove_spec(&mut self, pkg: &api::Ident) -> Result<()> {
        let versions = match self.specs.get_mut(pkg.name()) {
            Some(v) => v,
            None => {
                self.specs
                    .insert(pkg.name().to_string(), Default::default());
                self.specs.get_mut(pkg.name()).unwrap()
            }
        };
        let existing = versions.remove(&pkg.version);
        if existing.is_none() {
            Err(Error::PackageNotFoundError(pkg.clone()))
        } else {
            Ok(())
        }
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
