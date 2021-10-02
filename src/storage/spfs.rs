// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use itertools::Itertools;
use relative_path::RelativePathBuf;
use spfs::prelude::*;

use super::Repository;
use crate::{api, Digest, Error, Result};

#[derive(Debug)]
pub struct SPFSRepository {
    inner: spfs::storage::RepositoryHandle,
}

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
    fn list_packages(&self) -> Result<Vec<String>> {
        let path = relative_path::RelativePath::new("spk/spec");
        Ok(self
            .inner
            .ls_tags(&path)?
            .filter_map(|entry| {
                if entry.ends_with("/") {
                    Some(entry[0..entry.len() - 1].to_owned())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>> {
        let path = self.build_spec_tag(&api::parse_ident(name)?);
        let mut versions = self
            .inner
            .ls_tags(&path)?
            .filter_map(|entry| {
                if entry.ends_with("/") {
                    let stripped = &entry[0..entry.len() - 1];
                    // undo our encoding of the invalid '+' character in spfs tags
                    Some(stripped.replace("..", "+"))
                } else {
                    Some(entry.replace("..", "+"))
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
        base.pop();

        Ok(self
            .inner
            .ls_tags(&base)?
            .filter(|entry| !entry.ends_with("/"))
            .filter_map(|b| match api::parse_build(&b) {
                Ok(b) => Some(b),
                Err(_) => {
                    tracing::warn!("Invalid build found in spfs tags: {}", b);
                    None
                }
            })
            .map(|b| pkg.with_build(Some(b)))
            .collect())
    }

    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec> {
        let tag_path = self.build_spec_tag(&pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
        let tag = self.inner.resolve_tag(&tag_spec).map_err(|err| match err {
            spfs::Error::UnknownReference(_) => Error::PackageNotFoundError(pkg.clone()),
            err => err.into(),
        })?;

        let reader = self.inner.open_payload(&tag.target)?;
        Ok(serde_yaml::from_reader(reader)?)
    }

    fn get_package(&self, pkg: &api::Ident) -> Result<spfs::encoding::Digest> {
        let tag_path = self.build_package_tag(&pkg)?;
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path.as_str())?;
        let tag = self.inner.resolve_tag(&tag_spec).map_err(|err| match err {
            spfs::Error::UnknownReference(_) => Error::PackageNotFoundError(pkg.clone()),
            err => err.into(),
        })?;
        Ok(tag.target)
    }

    fn publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        if spec.pkg.build.is_some() {
            return Err(api::InvalidBuildError::new(
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
        let tag_path = self.build_spec_tag(&pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        match self.inner.remove_tag_stream(&tag_spec) {
            Err(spfs::Error::UnknownReference(_)) => Err(Error::PackageNotFoundError(pkg.clone())),
            Err(err) => Err(err.into()),
            Ok(_) => Ok(()),
        }
    }

    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()> {
        if let Some(api::Build::Embedded) = spec.pkg.build {
            return Err(api::InvalidBuildError::new(
                "Cannot publish embedded package".to_string(),
            ));
        }
        let tag_path = self.build_spec_tag(&spec.pkg);
        let tag_spec = spfs::tracking::TagSpec::parse(tag_path)?;

        let payload = serde_yaml::to_vec(&spec)?;
        let (digest, size) = self.inner.write_data(Box::new(&mut payload.as_slice()))?;
        let blob = spfs::graph::Blob {
            payload: digest.clone(),
            size: size,
        };
        self.inner.write_blob(blob)?;
        self.inner.push_tag(&tag_spec, &digest)?;
        Ok(())
    }

    fn publish_package(&mut self, spec: api::Spec, digest: spfs::encoding::Digest) -> Result<()> {
        #[cfg(test)]
        match self.read_spec(&spec.pkg.with_build(None)) {
            Err(Error::PackageNotFoundError(pkg)) => {
                return Err(Error::String(format!(
                    "[INTERNAL] version spec must be published before a specific build: {:?}",
                    pkg
                )))
            }
            _ => (),
        }

        let tag_path = self.build_package_tag(&spec.pkg)?;
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        self.force_publish_spec(spec)?;
        self.inner.push_tag(&tag_spec, &digest)?;
        Ok(())
    }

    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()> {
        let tag_path = self.build_package_tag(&pkg)?;
        let tag_spec = spfs::tracking::TagSpec::parse(&tag_path)?;
        match self.inner.remove_tag_stream(&tag_spec) {
            Err(spfs::Error::UnknownReference(_)) => Err(Error::PackageNotFoundError(pkg.clone())),
            Err(err) => Err(err.into()),
            Ok(_) => Ok(()),
        }
    }
}

impl SPFSRepository {
    /// Construct an spfs tag string to represent a binary package layer.
    fn build_package_tag(&self, pkg: &api::Ident) -> Result<RelativePathBuf> {
        if pkg.build.is_none() {
            return Err(api::InvalidBuildError::new(
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

    pub fn has_tag(&self, tag: &str) -> bool {
        match tag.parse() {
            Ok(tag) => self.inner.has_tag(&tag),
            Err(_) => false,
        }
    }

    pub fn has_digest(&self, digest: &Digest) -> bool {
        self.inner.has_object(&digest.inner)
    }

    pub fn push_digest(&self, digest: &Digest, dest: &mut Self) -> Result<()> {
        spfs::sync_ref(digest.inner.to_string(), &self.inner, &mut dest.inner)?;
        Ok(())
    }

    pub fn localize_digest(&self, digest: &Digest) -> Result<()> {
        let mut local_repo = spfs::load_config()?.get_repository()?.into();
        spfs::sync_ref(digest.inner.to_string(), &self.inner, &mut local_repo)?;
        Ok(())
    }

    pub fn flush(&mut self) -> Result<()> {
        match &mut self.inner {
            spfs::storage::RepositoryHandle::Tar(tar) => Ok(tar.flush()?),
            _ => Ok(()),
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
