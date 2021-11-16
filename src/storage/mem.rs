// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;

use super::Repository;
use crate::{api, Error, Result};

type ComponentMap = HashMap<api::Component, spfs::encoding::Digest>;
type BuildMap = HashMap<api::Build, (api::Spec, ComponentMap)>;

#[derive(Default, Clone, Debug)]
pub struct MemRepository {
    specs: HashMap<String, HashMap<api::Version, api::Spec>>,
    packages: HashMap<String, HashMap<api::Version, BuildMap>>,
}

impl std::hash::Hash for MemRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (self as *const _ as usize).hash(state)
    }
}

impl PartialEq for MemRepository {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for MemRepository {}

impl Repository for MemRepository {
    fn address(&self) -> url::Url {
        let address = format!("mem://{:x}", self as *const _ as usize);
        url::Url::parse(&address)
            .expect("[INTERNAL ERROR] hex address should always create a valid url")
    }

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

    fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        let build = match pkg.build.as_ref() {
            Some(b) => b,
            None => return Ok(Vec::new()),
        };
        Ok(self
            .packages
            .get(pkg.name())
            .and_then(|versions| versions.get(&pkg.version))
            .and_then(|builds| builds.get(&build))
            .map(|(_, build_map)| build_map)
            .map(|cmpts| cmpts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default())
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

    fn get_package(&self, pkg: &api::Ident) -> Result<ComponentMap> {
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
            None => return Err(Error::PackageNotFoundError(pkg.clone())),
        };
        if versions.remove(&pkg.version).is_none() {
            Err(Error::PackageNotFoundError(pkg.clone()))
        } else {
            Ok(())
        }
    }

    fn publish_package(&mut self, spec: api::Spec, components: ComponentMap) -> Result<()> {
        let build = match &spec.pkg.build {
            Some(b) => b.to_owned(),
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be published: {}",
                    spec.pkg
                )))
            }
        };

        let versions = match self.packages.get_mut(spec.pkg.name()) {
            Some(v) => v,
            None => {
                self.packages
                    .insert(spec.pkg.name().to_string(), Default::default());
                self.packages.get_mut(spec.pkg.name()).unwrap()
            }
        };

        let builds = match versions.get_mut(&spec.pkg.version) {
            Some(v) => v,
            None => {
                versions.insert(spec.pkg.version.clone(), Default::default());
                versions.get_mut(&spec.pkg.version).unwrap()
            }
        };

        builds.insert(build, (spec, components));
        Ok(())
    }

    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()> {
        let build = match &pkg.build {
            Some(b) => b,
            None => {
                return Err(Error::String(format!(
                    "Package must include a build in order to be removed: {}",
                    pkg
                )))
            }
        };

        let versions = match self.packages.get_mut(pkg.name()) {
            Some(v) => v,
            None => return Err(Error::PackageNotFoundError(pkg.clone())),
        };

        let builds = match versions.get_mut(&pkg.version) {
            Some(v) => v,
            None => return Err(Error::PackageNotFoundError(pkg.clone())),
        };

        if builds.remove(build).is_none() {
            Err(Error::PackageNotFoundError(pkg.clone()))
        } else {
            Ok(())
        }
    }
}
