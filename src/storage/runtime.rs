// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;

use spfs::prelude::*;
use tokio::runtime::Handle;

use super::Repository;
use crate::{api, Error, Result};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
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

impl RuntimeRepository {
    #[cfg(test)]
    pub fn new(root: std::path::PathBuf) -> Self {
        // this function is not allowed outside of testing because get_package
        // makes assumptions about the runtime directory which cannot be
        // reasonably altered
        Self { root }
    }
}

impl Repository for RuntimeRepository {
    fn address(&self) -> url::Url {
        let address = format!("runtime://{}", self.root.display());
        match url::Url::parse(&address) {
            Ok(a) => a,
            Err(err) => {
                tracing::error!(
                    "failed to create valid address for path {:?}: {:?}",
                    self.root,
                    err
                );
                url::Url::parse(&format!("runtime://{}", self.root.to_string_lossy()))
                    .expect("Failed to create url from path (fallback)")
            }
        }
    }

    fn list_packages(&self) -> Result<Vec<String>> {
        Ok(get_all_filenames(&self.root)?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .collect())
    }

    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        Ok(get_all_filenames(self.root.join(name))?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
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
        let mut base = self.root.join(pkg.name());
        base.push(pkg.version.to_string());
        Ok(get_all_filenames(&base)?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .filter(|entry| base.join(entry).join("spec.yaml").exists())
            .filter_map(|candidate| match api::parse_build(&candidate) {
                Ok(b) => Some(pkg.with_build(Some(b))),
                Err(err) => {
                    tracing::debug!(
                        "Skipping invalid build in {:?}: [{}] {:?}",
                        self.root,
                        candidate,
                        err
                    );
                    None
                }
            })
            .collect())
    }

    fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        if pkg.build.is_none() {
            return Ok(Vec::new());
        }
        let entries = get_all_filenames(self.root.join(pkg.to_string()))?;
        entries
            .into_iter()
            .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
            .map(api::Component::parse)
            .collect()
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        let mut path = self.root.join(pkg.to_string());
        path.push("spec.yaml");

        match api::read_spec_file(&path) {
            Err(Error::IO(err)) => {
                if err.kind() == std::io::ErrorKind::NotFound {
                    Err(Error::PackageNotFoundError(pkg.clone()))
                } else {
                    Err(err.into())
                }
            }
            err => err,
        }
    }

    fn get_package(
        &self,
        pkg: &api::Ident,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>> {
        let handle = Handle::current();
        let entries = get_all_filenames(self.root.join(pkg.to_string()))?;
        let components: Vec<api::Component> = entries
            .into_iter()
            .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
            .map(api::Component::parse)
            .collect::<Result<_>>()?;

        let mut path = relative_path::RelativePathBuf::from("/spk/pkg");
        path.push(pkg.to_string());
        let mut mapped = HashMap::with_capacity(components.len());
        for name in components.into_iter() {
            let digest = handle
                .block_on(find_layer_by_filename(path.join(format!("{}.cmpt", &name))))
                .map_err(|err| {
                    if let Error::SPFS(spfs::Error::UnknownReference(_)) = err {
                        Error::PackageNotFoundError(pkg.clone())
                    } else {
                        err
                    }
                })?;
            mapped.insert(name, digest);
        }
        Ok(mapped)
    }

    fn force_publish_spec(&self, _spec: api::Spec) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }

    fn publish_spec(&self, _spec: api::Spec) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    fn remove_spec(&self, _pkg: &api::Ident) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }

    fn publish_package(
        &self,
        _spec: api::Spec,
        _components: HashMap<api::Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    fn remove_package(&self, _pkg: &api::Ident) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
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

async fn find_layer_by_filename<S: AsRef<str>>(path: S) -> Result<spfs::encoding::Digest> {
    let runtime = spfs::active_runtime()?;
    let repo = spfs::load_config()?.get_repository().await?.into();

    let stack = runtime.get_stack();
    let layers = spfs::resolve_stack_to_layers(stack.iter(), Some(&repo)).await?;
    for layer in layers.iter().rev() {
        let manifest = repo.read_manifest(layer.manifest).await?.unlock();
        if manifest.get_path(&path).is_some() {
            return Ok(layer.digest()?);
        }
    }
    Err(spfs::Error::UnknownReference(path.as_ref().into()).into())
}
