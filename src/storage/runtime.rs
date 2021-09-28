// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;

use super::Repository;
use crate::{api, Result};

#[derive(Clone, Debug)]
pub struct RuntimeRepository {
    root: std::path::PathBuf,
}

impl Default for RuntimeRepository {
    fn default() -> Self {
        Self {
            root: std::path::PathBuf::from("/spfs/spk/pkg"),
        }
    }
}

impl Repository for RuntimeRepository {
    fn list_packages(&self) -> Result<Vec<String>> {
        // try:
        //     return os.listdir("/spfs/spk/pkg")
        // except FileNotFoundError:
        //     return []
        todo!()
    }

    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        // try:
        //     return os.listdir(f"/spfs/spk/pkg/{name}")
        // except FileNotFoundError:
        //     return []
        todo!()
    }

    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        // if isinstance(pkg, str):
        //     pkg = api.parse_ident(pkg)

        // try:
        //     builds = os.listdir(f"/spfs/spk/pkg/{pkg.name}/{pkg.version}")
        // except FileNotFoundError:
        //     return

        // for build in builds:
        //     if os.path.isfile(
        //         f"/spfs/spk/pkg/{pkg.name}/{pkg.version}/{build}/spec.yaml"
        //     ):
        //         yield pkg.with_build(build)
        todo!()
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        // try:
        //     spec_file = os.path.join("/spfs/spk/pkg", str(pkg), "spec.yaml")
        //     return api.read_spec_file(spec_file)
        // except FileNotFoundError:
        //     raise PackageNotFoundError(pkg)
        todo!()
    }

    fn get_package(&self, pkg: &api::Ident) -> Result<spfs::encoding::Digest> {
        // spec_path = os.path.join("/spk/pkg", str(pkg), "spec.yaml")
        // try:
        //     return spkrs.find_layer_by_filename(spec_path)
        // except RuntimeError:
        //     raise PackageNotFoundError(pkg)
        todo!()
    }

    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        // raise NotImplementedError("Cannot modify a runtime repository")
        todo!()
    }

    fn publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        // raise NotImplementedError("Cannot publish to a runtime repository")
        todo!()
    }

    fn remove_spec(&mut self, pkg: &api::Ident) -> Result<()> {
        // raise NotImplementedError("Cannot modify a runtime repository")
        todo!()
    }

    fn publish_package(&mut self, spec: api::Spec, digest: spfs::encoding::Digest) -> Result<()> {
        // raise NotImplementedError("Cannot publish to a runtime repository")
        todo!()
    }

    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()> {
        // raise NotImplementedError("Cannot modify a runtime repository")
        todo!()
    }
}
