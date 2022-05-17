// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use futures::{future::ready, TryStreamExt};
use itertools::Itertools;
use relative_path::RelativePathBuf;
use serde_derive::{Deserialize, Serialize};
use spfs::storage::EntryType;
use tokio::io::AsyncReadExt;
use tokio::runtime::Handle;

use super::Repository;
use crate::{api, Error, Result};

#[cfg(test)]
#[path = "./spfs_test.rs"]
mod spfs_test;

const REPO_METADATA_TAG: &str = "spk/repo";
const REPO_VERSION: &str = "1.0.0";

#[derive(Debug)]
pub struct SPFSRepository {
    inner: spfs::storage::RepositoryHandle,
}

impl std::hash::Hash for SPFSRepository {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.inner.address().hash(state);
    }
}

impl PartialEq for SPFSRepository {
    fn eq(&self, other: &Self) -> bool {
        self.inner.address() == other.inner.address()
    }
}

impl Eq for SPFSRepository {}

impl std::ops::Deref for SPFSRepository {
    type Target = spfs::storage::RepositoryHandle;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for SPFSRepository {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: Into<spfs::storage::RepositoryHandle>> From<T> for SPFSRepository {
    fn from(repo: T) -> Self {
        Self { inner: repo.into() }
    }
}

impl SPFSRepository {
    pub async fn new(address: &str) -> Result<Self> {
        Ok(Self {
            inner: spfs::open_repository(address).await?,
        })
    }
}

impl Repository for SPFSRepository {
    fn address(&self) -> url::Url {
        self.inner.address()
    }

    fn list_packages(&self) -> Result<Vec<api::PkgName>> {
        Handle::current().block_on(async {
            let path = relative_path::RelativePath::new("spk/spec");
            self.inner
                .ls_tags(path)
                .map_err(crate::Error::SPFS)
                .try_filter_map(|entry| {
                    ready(match entry {
                        EntryType::Folder(name) => name.parse().map(Some),
                        EntryType::Tag(_) => Ok(None),
                    })
                })
                .try_collect()
                .await
        })
    }

    fn list_package_versions(&self, name: &api::PkgName) -> Result<Vec<api::Version>> {
        Handle::current().block_on(async {
            let path = self.build_spec_tag(&name.clone().into());
            let versions: HashSet<_> = self
                .inner
                .ls_tags(&path)
                .try_filter_map(|entry| {
                    ready(Ok(Some(match entry {
                        // undo our encoding of the invalid '+' character in spfs tags
                        EntryType::Folder(name) => name.replace("..", "+"),
                        EntryType::Tag(name) => name.replace("..", "+"),
                    })))
                })
                .try_filter_map(|v| {
                    ready(match api::parse_version(&v) {
                        Ok(v) => Ok(Some(v)),
                        Err(_) => {
                            tracing::warn!("Invalid version found in spfs tags: {}", v);
                            Ok(None)
                        }
                    })
                })
                .try_collect()
                .await?;
            let mut versions = versions.into_iter().collect_vec();
            versions.sort();
            Ok(versions)
        })
    }

    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        Handle::current().block_on(async {
            let pkg = pkg.with_build(Some(api::Build::Source));
            let mut base = self.build_package_tag(&pkg)?;
            // the package tag contains the name and build, but we need to
            // remove the trailing build in order to list the containing 'folder'
            // eg: pkg/1.0.0/src => pkg/1.0.0
            base.pop();

            let builds: HashSet<_> = self
                .inner
                .ls_tags(&base)
                .try_filter_map(|tag| {
                    ready(match tag {
                        EntryType::Tag(name) => Ok(Some(name)),
                        EntryType::Folder(name) => Ok(Some(name)),
                    })
                })
                .try_filter_map(|b| {
                    ready(match api::parse_build(&b) {
                        Ok(b) => Ok(Some(b)),
                        Err(_) => {
                            tracing::warn!("Invalid build found in spfs tags: {}", b);
                            Ok(None)
                        }
                    })
                })
                .and_then(|b| ready(Ok(pkg.with_build(Some(b)))))
                .try_collect()
                .await?;
            Ok(builds.into_iter().collect_vec())
        })
    }

    fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        Handle::current().block_on(async {
            match self.lookup_package(pkg).await {
                Ok(p) => Ok(p.into_components().into_keys().collect()),
                Err(Error::PackageNotFoundError(_)) => Ok(Vec::new()),
                Err(err) => Err(err),
            }
        })
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        Handle::current().block_on(async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
            let tag = self
                .inner
                .resolve_tag(&tag_spec)
                .await
                .map_err(|err| match err {
                    spfs::Error::UnknownReference(_) => Error::PackageNotFoundError(pkg.clone()),
                    err => err.into(),
                })?;

            let mut reader = self.inner.open_payload(tag.target).await?;
            let mut yaml = String::new();
            reader.read_to_string(&mut yaml).await?;
            Ok(serde_yaml::from_str(&yaml)?)
        })
    }

    fn get_package(
        &self,
        pkg: &api::Ident,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>> {
        Handle::current().block_on(async {
            let package = self.lookup_package(pkg).await?;
            let component_tags = package.into_components();
            let mut components = HashMap::with_capacity(component_tags.len());
            for (name, tag_spec) in component_tags.into_iter() {
                let tag = self
                    .inner
                    .resolve_tag(&tag_spec)
                    .await
                    .map_err(|err| match err {
                        spfs::Error::UnknownReference(_) => {
                            Error::PackageNotFoundError(pkg.clone())
                        }
                        err => err.into(),
                    })?;
                components.insert(name, tag.target);
            }
            Ok(components)
        })
    }

    fn publish_spec(&self, spec: api::Spec) -> Result<()> {
        let spec = Handle::current().block_on(async {
            if spec.pkg.build.is_some() {
                return Err(api::InvalidBuildError::new_error(
                    "Spec must be published with no build".to_string(),
                ));
            }
            let tag_path = self.build_spec_tag(&spec.pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
            if self.inner.has_tag(&tag_spec).await {
                // BUG(rbottriell): this creates a race condition but is not super dangerous
                // because of the non-destructive tag history
                Err(Error::VersionExistsError(spec.pkg))
            } else {
                Ok(spec)
            }
        })?;
        self.force_publish_spec(spec)
    }

    fn remove_spec(&self, pkg: &api::Ident) -> Result<()> {
        Handle::current().block_on(async {
            let tag_path = self.build_spec_tag(pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
            match self.inner.remove_tag_stream(&tag_spec).await {
                Err(spfs::Error::UnknownReference(_)) => {
                    Err(Error::PackageNotFoundError(pkg.clone()))
                }
                Err(err) => Err(err.into()),
                Ok(_) => Ok(()),
            }
        })
    }

    fn force_publish_spec(&self, spec: api::Spec) -> Result<()> {
        Handle::current().block_on(async {
            if let Some(api::Build::Embedded) = spec.pkg.build {
                return Err(api::InvalidBuildError::new_error(
                    "Cannot publish embedded package".to_string(),
                ));
            }
            let tag_path = self.build_spec_tag(&spec.pkg);
            let tag_spec = spfs::tracking::TagSpec::parse(tag_path)?;

            let payload = serde_yaml::to_vec(&spec)?;
            let (digest, size) = self
                .inner
                .write_data(Box::pin(std::io::Cursor::new(payload)))
                .await?;
            let blob = spfs::graph::Blob {
                payload: digest,
                size,
            };
            self.inner.write_blob(blob).await?;
            self.inner.push_tag(&tag_spec, &digest).await?;
            Ok(())
        })
    }

    fn publish_package(
        &self,
        spec: api::Spec,
        components: HashMap<api::Component, spfs::encoding::Digest>,
    ) -> Result<()> {
        #[cfg(test)]
        if let Err(Error::PackageNotFoundError(pkg)) = self.read_spec(&spec.pkg.with_build(None)) {
            return Err(Error::String(format!(
                "[INTERNAL] version spec must be published before a specific build: {:?}",
                pkg
            )));
        }

        let tag_path = self.build_package_tag(&spec.pkg)?;

        // We will also publish the 'run' component in the old style
        // for compatibility with older versions of the spk command.
        // It's not perfect but at least the package will be visible
        let legacy_tag = spfs::tracking::TagSpec::parse(&tag_path)?;
        let legacy_component = if let Some(api::Build::Source) = spec.pkg.build {
            *components.get(&api::Component::Source).ok_or_else(|| {
                Error::String("Package must have a source component to be published".to_string())
            })?
        } else {
            *components.get(&api::Component::Run).ok_or_else(|| {
                Error::String("Package must have a run component to be published".to_string())
            })?
        };
        let spec: Result<api::Spec> = Handle::current().block_on(async {
            self.inner.push_tag(&legacy_tag, &legacy_component).await?;

            let components: std::result::Result<Vec<_>, _> = components
                .into_iter()
                .map(|(name, digest)| {
                    spfs::tracking::TagSpec::parse(tag_path.join(name.as_str()))
                        .map(|spec| (spec, digest))
                })
                .collect();
            for (tag_spec, digest) in components?.into_iter() {
                self.inner.push_tag(&tag_spec, &digest).await?;
            }
            Ok(spec)
        });

        self.force_publish_spec(spec?)?;
        Ok(())
    }

    fn remove_package(&self, pkg: &api::Ident) -> Result<()> {
        Handle::current().block_on(async {
            for tag_spec in self.lookup_package(pkg).await?.tags() {
                match self.inner.remove_tag_stream(tag_spec).await {
                    Err(spfs::Error::UnknownReference(_)) => (),
                    res => res?,
                }
            }
            // because we double-publish packages to be visible/compatible
            // with the old repo tag structure, we must also try to remove
            // the legacy version of the tag after removing the discovered
            // as it may still be there and cause the removal to be ineffective
            if let Ok(legacy_tag) = spfs::tracking::TagSpec::parse(self.build_package_tag(pkg)?) {
                match self.inner.remove_tag_stream(&legacy_tag).await {
                    Err(spfs::Error::UnknownReference(_)) => (),
                    res => res?,
                }
            }
            Ok(())
        })
    }

    fn upgrade(&self) -> Result<String> {
        let target_version = crate::api::Version::from_str(REPO_VERSION).unwrap();
        let mut meta = Handle::current().block_on(self.read_metadata())?;
        if meta.version > target_version {
            // for this particular upgrade (moving old-style tags to new)
            // we allow it to be run again over the same repo since it's
            // possible that some clients are still publishing the old way
            // during the transition period
            return Ok("Nothing to do.".to_string());
        }
        for name in self.list_packages()? {
            tracing::info!("replicating old tags for {}...", name);
            let mut pkg = api::Ident::new(name.to_owned());
            for version in self.list_package_versions(&name)? {
                pkg.version = version;
                for build in self.list_package_builds(&pkg)? {
                    let stored = crate::HANDLE.block_on(self.lookup_package(&build))?;
                    if stored.has_components() {
                        continue;
                    }
                    let components = stored.into_components();
                    for (name, tag_spec) in components.into_iter() {
                        let tag = crate::HANDLE.block_on(self.resolve_tag(&tag_spec))?;
                        let new_tag_path = self.build_package_tag(&build)?.join(name.to_string());
                        let new_tag_spec = spfs::tracking::TagSpec::parse(&new_tag_path)?;

                        // NOTE(rbottriell): this copying process feels annoying
                        // and error prone. Ideally, there would be some set methods
                        // on the tag for changing the org/name on an existing one
                        let mut new_tag = spfs::tracking::Tag::new(
                            new_tag_spec.org(),
                            new_tag_spec.name(),
                            tag.target,
                        )?;
                        new_tag.parent = tag.parent;
                        new_tag.time = tag.time;
                        new_tag.user = tag.user;

                        crate::HANDLE.block_on(self.push_raw_tag(&new_tag))?;
                    }
                }
            }
        }
        meta.version = target_version;
        crate::HANDLE.block_on(self.write_metadata(&meta))?;
        Ok("All packages were re-tagged for components".to_string())
    }
}

impl SPFSRepository {
    /// Read the metadata for this spk repository.
    ///
    /// The repo metadata contains information about
    /// how this particular spfs repository has been setup
    /// with spk. Namely, version and compatibility information.
    pub async fn read_metadata(&self) -> Result<RepositoryMetadata> {
        let tag_spec = spfs::tracking::TagSpec::parse(REPO_METADATA_TAG).unwrap();
        let digest = match self.inner.resolve_tag(&tag_spec).await {
            Ok(tag) => tag.target,
            Err(spfs::Error::UnknownReference(_)) => return Ok(Default::default()),
            Err(err) => return Err(err.into()),
        };
        let mut reader = self.inner.open_payload(digest).await?;
        let mut yaml = String::new();
        reader.read_to_string(&mut yaml).await?;
        let meta: RepositoryMetadata = serde_yaml::from_str(&yaml)?;
        Ok(meta)
    }

    /// Update the metadata for this spk repository.
    async fn write_metadata(&self, meta: &RepositoryMetadata) -> Result<()> {
        let tag_spec = spfs::tracking::TagSpec::parse(REPO_METADATA_TAG).unwrap();
        let yaml = serde_yaml::to_string(meta)?;
        let (digest, _size) = self
            .inner
            .write_data(Box::pin(std::io::Cursor::new(yaml.into_bytes())))
            .await?;
        self.inner.push_tag(&tag_spec, &digest).await?;
        Ok(())
    }

    /// Find a package stored in this repo in either the new or old way of tagging
    ///
    /// (with or without package components)
    async fn lookup_package(&self, pkg: &api::Ident) -> Result<StoredPackage> {
        use api::Component;
        use spfs::tracking::TagSpec;
        let tag_path = self.build_package_tag(pkg)?;
        let tag_specs: HashMap<Component, TagSpec> = self
            .inner
            .ls_tags(&tag_path)
            .try_filter_map(|e| {
                ready(match e {
                    EntryType::Tag(name) => Ok(Some(name)),
                    EntryType::Folder(_) => Ok(None),
                })
            })
            .try_filter_map(|e| ready(Ok(Component::parse(&e).map(|c| (c, e)).ok())))
            .try_filter_map(|(c, e)| {
                ready(Ok(TagSpec::parse(&tag_path.join(e)).map(|p| (c, p)).ok()))
            })
            .try_collect()
            .await?;
        if !tag_specs.is_empty() {
            return Ok(StoredPackage::WithComponents(tag_specs));
        }
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        if self.inner.has_tag(&tag_spec).await {
            return Ok(StoredPackage::WithoutComponents(tag_spec));
        }
        Err(Error::PackageNotFoundError(pkg.clone()))
    }

    /// Construct an spfs tag string to represent a binary package layer.
    fn build_package_tag(&self, pkg: &api::Ident) -> Result<RelativePathBuf> {
        if pkg.build.is_none() {
            return Err(api::InvalidBuildError::new_error(
                "Package must have associated build digest".to_string(),
            ));
        }

        let mut tag = RelativePathBuf::from("spk");
        tag.push("pkg");
        // the "+" character is not a valid spfs tag character,
        // so we 'encode' it with two dots, which is not a valid sequence
        // for spk package names
        tag.push(pkg.to_string().replace('+', ".."));

        Ok(tag)
    }

    /// Construct an spfs tag string to represent a spec file blob.
    fn build_spec_tag(&self, pkg: &api::Ident) -> RelativePathBuf {
        let mut tag = RelativePathBuf::from("spk");
        tag.push("spec");
        // the "+" character is not a valid spfs tag character,
        // see above ^
        tag.push(pkg.to_string().replace('+', ".."));

        tag
    }

    pub fn flush(&mut self) -> Result<()> {
        match &mut self.inner {
            spfs::storage::RepositoryHandle::Tar(tar) => Ok(tar.flush()?),
            _ => Ok(()),
        }
    }
}

#[derive(Deserialize, Serialize, Default, Debug, PartialEq, Eq)]
pub struct RepositoryMetadata {
    version: api::Version,
}

/// A simple enum that allows us to represent both the old and new form
/// of package storage as spfs tags.
enum StoredPackage {
    WithoutComponents(spfs::tracking::TagSpec),
    WithComponents(HashMap<api::Component, spfs::tracking::TagSpec>),
}

impl StoredPackage {
    /// true if this stored package uses the new format with
    /// tags for each package component
    fn has_components(&self) -> bool {
        matches!(self, Self::WithComponents(_))
    }

    /// Identify all of the tags associated with this package
    fn tags(&self) -> Vec<&spfs::tracking::TagSpec> {
        match &self {
            Self::WithoutComponents(tag) => vec![tag],
            Self::WithComponents(cmpts) => cmpts.values().collect(),
        }
    }

    /// Return the mapped component tags for this package, converting
    /// from the legacy storage format if needed.
    fn into_components(self) -> HashMap<api::Component, spfs::tracking::TagSpec> {
        use api::Component;
        match self {
            Self::WithComponents(cmpts) => cmpts,
            Self::WithoutComponents(tag) if tag.name() == "src" => {
                vec![(Component::Source, tag)].into_iter().collect()
            }
            Self::WithoutComponents(tag) => {
                vec![(Component::Build, tag.clone()), (Component::Run, tag)]
                    .into_iter()
                    .collect()
            }
        }
    }
}

/// Return the local packages repository used for development.
pub async fn local_repository() -> Result<SPFSRepository> {
    let config = spfs::get_config()?;
    let repo = config.get_repository().await?;
    Ok(SPFSRepository { inner: repo.into() })
}

/// Return the remote repository of the given name.
///
/// If not name is specified, return the default spfs repository.
pub async fn remote_repository<S: AsRef<str>>(name: S) -> Result<SPFSRepository> {
    let config = spfs::get_config()?;
    let repo = config.get_remote(name).await?;
    Ok(SPFSRepository { inner: repo })
}
