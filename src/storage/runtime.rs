// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{HashMap, HashSet},
    convert::{TryFrom, TryInto},
    sync::Arc,
};

use spfs::prelude::*;

use super::{repository::Storage, Repository};
use crate::{api, Error, Result};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RuntimeRepository {
    address: url::Url,
    name: api::RepositoryNameBuf,
    root: std::path::PathBuf,
}

impl Default for RuntimeRepository {
    fn default() -> Self {
        let root = std::path::PathBuf::from("/spfs/spk/pkg");
        let address = Self::address_from_root(&root);
        Self {
            address,
            name: "runtime".try_into().expect("valid repository name"),
            root,
        }
    }
}

impl Ord for RuntimeRepository {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}
impl PartialOrd for RuntimeRepository {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl RuntimeRepository {
    fn address_from_root(root: &std::path::PathBuf) -> url::Url {
        let address = format!("runtime://{}", root.display());
        match url::Url::parse(&address) {
            Ok(a) => a,
            Err(err) => {
                tracing::error!(
                    "failed to create valid address for path {:?}: {:?}",
                    root,
                    err
                );
                url::Url::parse(&format!("runtime://{}", root.to_string_lossy()))
                    .expect("Failed to create url from path (fallback)")
            }
        }
    }

    #[cfg(test)]
    pub fn new(root: std::path::PathBuf) -> Self {
        // this function is not allowed outside of testing because read_components
        // makes assumptions about the runtime directory which cannot be
        // reasonably altered
        let address = Self::address_from_root(&root);
        Self {
            address,
            name: "runtime".try_into().expect("valid repository name"),
            root,
        }
    }
}

#[async_trait::async_trait]
impl Storage for RuntimeRepository {
    type Recipe = api::SpecRecipe;
    type Package = api::Spec;

    async fn get_concrete_package_builds(&self, pkg: &api::Ident) -> Result<HashSet<api::Ident>> {
        let mut base = self.root.join(&pkg.name);
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

    async fn get_embedded_package_builds(&self, _pkg: &api::Ident) -> Result<HashSet<api::Ident>> {
        // Can't publish packages to a runtime so there can't be any stubs
        Ok(HashSet::default())
    }

    async fn read_components_from_storage(
        &self,
        pkg: &api::Ident,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>> {
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
            let digest = find_layer_by_filename(path.join(format!("{}.cmpt", &name)))
                .await
                .map_err(|err| {
                    if let Error::SPFS(spfs::Error::UnknownReference(_)) = err {
                        Error::PackageNotFoundError(pkg.clone())
                    } else {
                        err
                    }
                })?;
            mapped.insert(name, digest);
        }

        if mapped.is_empty() {
            // This is package was published before component support
            // was added. It does not have distinct components. So add
            // default Build and Run components that point at the full
            // package digest.
            let digest = find_layer_by_filename(path).await.map_err(|err| {
                if let Error::SPFS(spfs::Error::UnknownReference(_)) = err {
                    Error::PackageNotFoundError(pkg.clone())
                } else {
                    err
                }
            })?;
            mapped.insert(api::Component::Run, digest);
            mapped.insert(api::Component::Build, digest);
        }
        Ok(mapped)
    }

    async fn publish_package_to_storage(
        &self,
        _package: &<Self::Recipe as api::Recipe>::Output,
        _components: &HashMap<api::Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn publish_recipe_to_storage(&self, _spec: &Self::Recipe, _force: bool) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn remove_package_from_storage(&self, _pkg: &api::Ident) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }
}

#[async_trait::async_trait]
impl Repository for RuntimeRepository {
    fn address(&self) -> &url::Url {
        &self.address
    }

    fn name(&self) -> &api::RepositoryName {
        &self.name
    }

    async fn list_packages(&self) -> Result<Vec<api::PkgNameBuf>> {
        Ok(get_all_filenames(&self.root)?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .filter_map(|e| api::PkgNameBuf::try_from(e).ok())
            .collect())
    }

    async fn list_package_versions(
        &self,
        name: &api::PkgName,
    ) -> Result<Arc<Vec<Arc<api::Version>>>> {
        Ok(Arc::new(
            get_all_filenames(self.root.join(name))?
                .into_iter()
                .filter_map(|entry| {
                    if entry.ends_with('/') {
                        Some(entry[0..entry.len() - 1].to_string())
                    } else {
                        None
                    }
                })
                .filter_map(|candidate| match api::parse_version(&candidate) {
                    Ok(v) => Some(Arc::new(v)),
                    Err(err) => {
                        tracing::debug!(
                            "Skipping invalid version in /spfs/spk: [{}], {:?}",
                            candidate,
                            err
                        );
                        None
                    }
                })
                .collect(),
        ))
    }

    async fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
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

    async fn read_recipe(&self, pkg: &api::Ident) -> Result<Arc<Self::Recipe>> {
        Err(Error::PackageNotFoundError(pkg.clone()))
    }

    async fn remove_recipe(&self, _pkg: &api::Ident) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }

    async fn read_package(
        &self,
        pkg: &api::Ident,
    ) -> Result<Arc<<Self::Recipe as api::Recipe>::Output>> {
        let mut path = self.root.join(pkg.to_string());
        path.push("spec.yaml");

        let mut reader = std::fs::File::open(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::PackageNotFoundError(pkg.clone())
            } else {
                err.into()
            }
        })?;
        serde_yaml::from_reader(&mut reader)
            .map(Arc::new)
            .map_err(|err| Error::InvalidPackageSpec(pkg.clone(), err))
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
    let config = spfs::get_config()?;
    let (runtime, repo) =
        tokio::try_join!(spfs::active_runtime(), config.get_local_repository_handle())?;

    let layers = spfs::resolve_stack_to_layers(runtime.status.stack.iter(), Some(&repo)).await?;
    for layer in layers.iter().rev() {
        let manifest = repo.read_manifest(layer.manifest).await?.unlock();
        if manifest.get_path(&path).is_some() {
            return Ok(layer.digest()?);
        }
    }
    Err(spfs::Error::UnknownReference(path.as_ref().into()).into())
}
