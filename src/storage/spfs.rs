// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;

use itertools::Itertools;
use relative_path::RelativePathBuf;

use super::Repository;
use crate::{api, Error, Result};

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

impl From<spfs::storage::RepositoryHandle> for SPFSRepository {
    fn from(repo: spfs::storage::RepositoryHandle) -> Self {
        Self { inner: repo }
    }
}

impl SPFSRepository {
    pub fn new(address: &str) -> Result<Self> {
        Ok(Self {
            inner: spfs::storage::open_repository(address)?,
        })
    }
}

impl Repository for SPFSRepository {
    fn address(&self) -> url::Url {
        self.inner.address()
    }

    fn list_packages(&self) -> Result<Vec<String>> {
        let path = relative_path::RelativePath::new("spk/spec");
        Ok(self
            .inner
            .ls_tags(path)?
            .filter_map(|entry| {
                if entry.ends_with('/') {
                    Some(entry[0..entry.len() - 1].to_owned())
                } else {
                    None
                }
            })
            .collect_vec())
    }

    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        let path = self.build_spec_tag(&api::parse_ident(name)?);
        let mut versions = self
            .inner
            .ls_tags(&path)?
            .map(|entry| {
                if entry.ends_with('/') {
                    let stripped = &entry[0..entry.len() - 1];
                    // undo our encoding of the invalid '+' character in spfs tags
                    stripped.replace("..", "+")
                } else {
                    entry.replace("..", "+")
                }
            })
            .filter_map(|v| match api::parse_version(&v) {
                Ok(v) => Some(v),
                Err(_) => {
                    tracing::warn!("Invalid version found in spfs tags: {}", v);
                    None
                }
            })
            .unique()
            .collect_vec();
        versions.sort();
        Ok(versions)
    }

    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>> {
        let pkg = pkg.with_build(Some(api::Build::Source));
        let mut base = self.build_package_tag(&pkg)?;
        // the package tag contains the name and build, but we need to
        // remove the trailing build in order to list the containing 'folder'
        // eg: pkg/1.0.0/src => pkg/1.0.0
        base.pop();

        Ok(self
            .inner
            .ls_tags(&base)?
            .map(|mut entry| {
                if entry.ends_with('/') {
                    entry.truncate(entry.len() - 1)
                }
                entry
            })
            .filter_map(|b| match api::parse_build(&b) {
                Ok(b) => Some(b),
                Err(_) => {
                    tracing::warn!("Invalid build found in spfs tags: {}", b);
                    None
                }
            })
            .map(|b| pkg.with_build(Some(b)))
            .unique()
            .collect())
    }

    fn list_build_components(&self, pkg: &api::Ident) -> Result<Vec<api::Component>> {
        match self.lookup_package(pkg) {
            Ok(p) => Ok(p.into_components().into_keys().collect()),
            Err(Error::PackageNotFoundError(_)) => Ok(Vec::new()),
            Err(err) => Err(err),
        }
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        let tag_path = self.build_spec_tag(pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
        let tag = self.inner.resolve_tag(&tag_spec).map_err(|err| match err {
            spfs::Error::UnknownReference(_) => Error::PackageNotFoundError(pkg.clone()),
            err => err.into(),
        })?;

        let reader = self.inner.open_payload(&tag.target)?;
        Ok(serde_yaml::from_reader(reader)?)
    }

    fn get_package(
        &self,
        pkg: &api::Ident,
    ) -> Result<HashMap<api::Component, spfs::encoding::Digest>> {
        let package = self.lookup_package(pkg)?;
        package
            .into_components()
            .into_iter()
            .map(|(name, tag_spec)| {
                self.inner
                    .resolve_tag(&tag_spec)
                    .map(|t| (name, t.target))
                    .map_err(|err| match err {
                        spfs::Error::UnknownReference(_) => {
                            Error::PackageNotFoundError(pkg.clone())
                        }
                        err => err.into(),
                    })
            })
            .collect()
    }

    fn publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        if spec.pkg.build.is_some() {
            return Err(api::InvalidBuildError::new_error(
                "Spec must be published with no build".to_string(),
            ));
        }
        let tag_path = self.build_spec_tag(&spec.pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
        if self.inner.has_tag(&tag_spec) {
            // BUG(rbottriell): this creates a race condition but is not super dangerous
            // because of the non-destructive tag history
            Err(Error::VersionExistsError(spec.pkg))
        } else {
            self.force_publish_spec(spec)
        }
    }

    fn remove_spec(&mut self, pkg: &api::Ident) -> Result<()> {
        let tag_path = self.build_spec_tag(pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        match self.inner.remove_tag_stream(&tag_spec) {
            Err(spfs::Error::UnknownReference(_)) => Err(Error::PackageNotFoundError(pkg.clone())),
            Err(err) => Err(err.into()),
            Ok(_) => Ok(()),
        }
    }

    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        if let Some(api::Build::Embedded) = spec.pkg.build {
            return Err(api::InvalidBuildError::new_error(
                "Cannot publish embedded package".to_string(),
            ));
        }
        let tag_path = self.build_spec_tag(&spec.pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path)?;

        let payload = serde_yaml::to_vec(&spec)?;
        let (digest, size) = self.inner.write_data(Box::new(&mut payload.as_slice()))?;
        let blob = spfs::graph::Blob {
            payload: digest,
            size,
        };
        self.inner.write_blob(blob)?;
        self.inner.push_tag(&tag_spec, &digest)?;
        Ok(())
    }

    fn publish_package(
        &mut self,
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
        let components: std::result::Result<Vec<_>, _> = components
            .into_iter()
            .map(|(name, digest)| {
                spfs::tracking::TagSpec::parse(tag_path.join(name.as_str()))
                    .map(|spec| (spec, digest))
            })
            .collect();
        for (tag_spec, digest) in components?.into_iter() {
            self.inner.push_tag(&tag_spec, &digest)?;
        }
        self.force_publish_spec(spec)?;
        Ok(())
    }

    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()> {
        for tag_spec in self.lookup_package(pkg)?.tags() {
            match self.inner.remove_tag_stream(tag_spec) {
                Err(spfs::Error::UnknownReference(_)) => (),
                res => res?,
            }
        }
        Ok(())
    }
}

impl SPFSRepository {
    /// Find a package stored in this repo in either the new or old way of tagging
    ///
    /// (with or without package components)
    fn lookup_package(&self, pkg: &api::Ident) -> Result<StoredPackage> {
        use api::Component;
        use spfs::tracking::TagSpec;
        let tag_path = self.build_package_tag(pkg)?;
        let tag_specs: HashMap<Component, TagSpec> = self
            .inner
            .ls_tags(&tag_path)?
            .filter(|e| !e.ends_with('/'))
            .filter_map(|e| Component::parse(&e).map(|c| (c, e)).ok())
            .filter_map(|(c, e)| TagSpec::parse(&tag_path.join(e)).map(|p| (c, p)).ok())
            .collect();
        if !tag_specs.is_empty() {
            return Ok(StoredPackage::WithComponents(tag_specs));
        }
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        if self.inner.has_tag(&tag_spec) {
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
        tag.push(pkg.to_string().replace("+", ".."));

        Ok(tag)
    }

    /// Construct an spfs tag string to represent a spec file blob.
    fn build_spec_tag(&self, pkg: &api::Ident) -> RelativePathBuf {
        let mut tag = RelativePathBuf::from("spk");
        tag.push("spec");
        // the "+" character is not a valid spfs tag character,
        // see above ^
        tag.push(pkg.to_string().replace("+", ".."));

        tag
    }

    pub fn flush(&mut self) -> Result<()> {
        match &mut self.inner {
            spfs::storage::RepositoryHandle::Tar(tar) => Ok(tar.flush()?),
            _ => Ok(()),
        }
    }
}

/// A simple enum that allows us to represent both the old and new form
/// of package storage as spfs tags.
enum StoredPackage {
    WithoutComponents(spfs::tracking::TagSpec),
    WithComponents(HashMap<api::Component, spfs::tracking::TagSpec>),
}

impl StoredPackage {
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
pub fn local_repository() -> Result<SPFSRepository> {
    let config = spfs::load_config()?;
    let repo = config.get_repository()?;
    Ok(SPFSRepository { inner: repo.into() })
}

/// Return the remote repository of the given name.
///
/// If not name is specified, return the default spfs repository.
pub fn remote_repository<S: AsRef<str>>(name: S) -> Result<SPFSRepository> {
    let config = spfs::load_config()?;
    let repo = config.get_remote(name)?;
    Ok(SPFSRepository { inner: repo })
}
