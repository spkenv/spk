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
        Ok(get_all_filenames("/spfs/spk/pkg")?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with("/") {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .collect())
    }

    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        Ok(get_all_filenames(format!("/spfs/spk/pkg/{}", name))?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with("/") {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .filter_map(|candidate| match api::parse_version(&candidate) {
                Ok(v) => Some(v),
                Err(err) => {
                    tracing::debug!(
                        "Skipping invalid version in /spfs/spk: [{}], {:?}",
                        candidate,
                        err
                    );
                    None
                }
            })
            .collect())
    }

    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        Ok(
            get_all_filenames(format!("/spfs/spk/pkg/{}/{}", pkg.name(), pkg.version))?
                .into_iter()
                .filter_map(|entry| {
                    if entry.ends_with("/") {
                        Some(entry[0..entry.len() - 1].to_string())
                    } else {
                        None
                    }
                })
                .filter_map(|candidate| match api::parse_build(&candidate) {
                    Ok(b) => Some(pkg.with_build(Some(b))),
                    Err(err) => {
                        tracing::debug!(
                            "Skipping invalid build in /spfs/spk: [{}] {:?}",
                            candidate,
                            err
                        );
                        None
                    }
                })
                .collect(),
        )
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

/// Works like ls_tags, returning strings that end with '/' for directories
/// and not for regular files
fn get_all_filenames<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(path) {
        Err(err) => {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(Default::default()),
                _ => Err(err.into()),
            }
        }
        Ok(e) => e.collect::<std::io::Result<Vec<_>>>(),
    };
    Ok(entries?
        .into_iter()
        .map(|entry| {
            let mut name = entry.file_name().to_string_lossy().to_string();
            match entry.file_type() {
                Ok(t) if t.is_dir() => name.push('/'),
                _ => (),
            }
            name
        })
        .collect())
}
