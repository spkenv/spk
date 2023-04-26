// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::convert::{TryFrom, TryInto};
use std::io::Read;
use std::sync::Arc;

use spfs::find_path::ObjectPathEntry;
use spfs::io::DigestFormat;
use spfs::prelude::*;
use spk_schema::foundation::ident_build::parse_build;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::name::{PkgName, PkgNameBuf, RepositoryName, RepositoryNameBuf};
use spk_schema::foundation::version::{parse_version, Version};
use spk_schema::{BuildIdent, FromYaml, Spec, SpecRecipe, VersionIdent};

use super::repository::{PublishPolicy, Storage};
use super::Repository;
use crate::{Error, Result};

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RuntimeRepository {
    address: url::Url,
    name: RepositoryNameBuf,
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
    type Recipe = SpecRecipe;
    type Package = Spec;

    async fn get_concrete_package_builds(&self, pkg: &VersionIdent) -> Result<HashSet<BuildIdent>> {
        let mut base = self.root.join(pkg.name());
        base.push(pkg.version().to_string());
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
            .filter_map(|candidate| match parse_build(&candidate) {
                Ok(b) => Some(pkg.to_build(b)),
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

    async fn get_embedded_package_builds(
        &self,
        _pkg: &VersionIdent,
    ) -> Result<HashSet<BuildIdent>> {
        // Can't publish packages to a runtime so there can't be any stubs
        Ok(HashSet::default())
    }

    async fn publish_embed_stub_to_storage(&self, _spec: &Self::Package) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn publish_package_to_storage(
        &self,
        _package: &<Self::Recipe as spk_schema::Recipe>::Output,
        _components: &HashMap<Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn publish_recipe_to_storage(
        &self,
        _spec: &Self::Recipe,
        _publish_policy: PublishPolicy,
    ) -> Result<()> {
        Err(Error::String(
            "Cannot publish to a runtime repository".into(),
        ))
    }

    async fn read_components_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<HashMap<Component, spfs::encoding::Digest>> {
        let entries = get_all_filenames(self.root.join(pkg.to_string()))?;
        let components: Vec<Component> = entries
            .into_iter()
            .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
            .map(Component::parse)
            .collect::<spk_schema::foundation::ident_component::Result<_>>()?;

        let mut path = relative_path::RelativePathBuf::from("/spk/pkg");
        path.push(pkg.to_string());

        let mut mapped = HashMap::with_capacity(components.len());
        for name in components.into_iter() {
            let digest = find_layer_by_filename(path.join(format!("{}.cmpt", &name)))
                .await
                .map_err(|err| {
                    if let Error::SPFS(spfs::Error::UnknownReference(_)) = err {
                        Error::SpkValidatorsError(
                            spk_schema::validators::Error::PackageNotFoundError(pkg.to_any()),
                        )
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
                    Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                        pkg.to_any(),
                    ))
                } else {
                    err
                }
            })?;
            mapped.insert(Component::Run, digest);
            mapped.insert(Component::Build, digest);
        }
        Ok(mapped)
    }

    async fn read_package_from_storage(
        &self,
        pkg: &BuildIdent,
    ) -> Result<Arc<<Self::Recipe as spk_schema::Recipe>::Output>> {
        let mut path = self.root.join(pkg.to_string());
        path.push("spec.yaml");

        let mut reader = std::fs::File::open(&path).map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                Error::SpkValidatorsError(spk_schema::validators::Error::PackageNotFoundError(
                    pkg.to_any(),
                ))
            } else {
                Error::FileOpenError(path.to_owned(), err)
            }
        })?;
        let mut yaml = String::new();
        reader
            .read_to_string(&mut yaml)
            .map_err(|err| Error::FileReadError(path.to_owned(), err))?;
        <Self::Recipe as spk_schema::Recipe>::Output::from_yaml(yaml)
            .map(Arc::new)
            .map_err(|err| Error::InvalidPackageSpec(pkg.to_any(), err.to_string()))
    }

    async fn remove_embed_stub_from_storage(&self, _pkg: &BuildIdent) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }

    async fn remove_package_from_storage(&self, _pkg: &BuildIdent) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }
}

#[async_trait::async_trait]
impl Repository for RuntimeRepository {
    fn address(&self) -> &url::Url {
        &self.address
    }

    fn name(&self) -> &RepositoryName {
        &self.name
    }

    async fn list_packages(&self) -> Result<Vec<PkgNameBuf>> {
        Ok(get_all_filenames(&self.root)?
            .into_iter()
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_string())
                } else {
                    None
                }
            })
            .filter_map(|e| PkgNameBuf::try_from(e).ok())
            .collect())
    }

    async fn list_package_versions(&self, name: &PkgName) -> Result<Arc<Vec<Arc<Version>>>> {
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
                .filter_map(|candidate| match parse_version(&candidate) {
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

    async fn list_build_components(&self, pkg: &BuildIdent) -> Result<Vec<Component>> {
        let entries = get_all_filenames(self.root.join(pkg.to_string()))?;
        entries
            .into_iter()
            .filter_map(|n| n.strip_suffix(".cmpt").map(str::to_string))
            .map(|c| Component::parse(c).map_err(|err| err.into()))
            .collect()
    }

    async fn read_embed_stub(&self, pkg: &BuildIdent) -> Result<Arc<Self::Package>> {
        Err(Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(pkg.to_any()),
        ))
    }

    async fn read_recipe(&self, pkg: &VersionIdent) -> Result<Arc<Self::Recipe>> {
        Err(Error::SpkValidatorsError(
            spk_schema::validators::Error::PackageNotFoundError(pkg.to_any(None)),
        ))
    }

    async fn remove_recipe(&self, _pkg: &VersionIdent) -> Result<()> {
        Err(Error::String("Cannot modify a runtime repository".into()))
    }
}

/// Works like ls_tags, returning strings that end with '/' for directories
/// and not for regular files
fn get_all_filenames<P: AsRef<std::path::Path>>(path: P) -> Result<Vec<String>> {
    let entries = match std::fs::read_dir(&path) {
        Err(err) => {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(Default::default()),
                _ => Err(Error::FileOpenError(path.as_ref().to_owned(), err)),
            }
        }
        Ok(e) => e.collect::<std::io::Result<Vec<_>>>(),
    };
    Ok(entries
        .map_err(|err| Error::FileOpenError(path.as_ref().to_owned(), err))?
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
        let manifest = repo
            .read_manifest(layer.manifest)
            .await?
            .to_tracking_manifest();
        if manifest.get_path(&path).is_some() {
            return Ok(layer.digest()?);
        }
    }
    Err(spfs::Error::UnknownReference(path.as_ref().into()).into())
}

/// Return a list of spfs object lists that lead to the given
/// filepath in the runtime repo.
pub async fn find_path_providers(filepath: &str) -> Result<(bool, Vec<Vec<ObjectPathEntry>>)> {
    let config = spfs::get_config()?;
    let repo = config.get_local_repository_handle().await?;

    spfs::find_path::find_path_providers_in_spfs_runtime(filepath, &repo)
        .await
        .map_err(|err| err.into())
}

/// Print out a spfs object list down to the given filepath
pub async fn pretty_print_filepath(
    filepath: &str,
    objectpath: &Vec<ObjectPathEntry>,
) -> Result<()> {
    let config = spfs::get_config()?;
    let repo = config.get_local_repository_handle().await?;

    let digest_format = DigestFormat::Shortened(&repo);
    match spfs::io::pretty_print_filepath(filepath, &objectpath, digest_format).await {
        Ok(r) => Ok(r),
        Err(err) => Err(err.into()),
    }
}
