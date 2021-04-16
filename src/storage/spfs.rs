// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pyo3::prelude::*;
use spfs;

use crate::{Digest, Result};

#[pyclass(subclass)]
pub struct SpFSRepository {
    inner: spfs::storage::RepositoryHandle,
}

impl From<spfs::storage::RepositoryHandle> for SpFSRepository {
    fn from(repo: spfs::storage::RepositoryHandle) -> Self {
        Self { inner: repo }
    }
}

#[pymethods]
impl SpFSRepository {
    #[new]
    pub fn new(address: &str) -> Result<Self> {
        Ok(Self {
            inner: spfs::storage::open_repository(address)?,
        })
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

    pub fn push_ref(&self, reference: &str, dest: &mut Self) -> Result<()> {
        spfs::sync_ref(reference, &self.inner, &mut dest.inner)?;
        Ok(())
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

    pub fn resolve_tag_to_digest(&self, tag: &str) -> Result<Option<Digest>> {
        let tag = tag.parse()?;
        match self.inner.resolve_tag(&tag) {
            Ok(tag) => Ok(Some(tag.target.into())),
            Err(spfs::Error::UnknownReference(_)) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    pub fn push_tag(&mut self, tag: &str, target: &Digest) -> Result<()> {
        let tag = tag.parse()?;
        self.inner.push_tag(&tag, &target.inner)?;
        Ok(())
    }

    pub fn ls_all_tags(&self) -> Result<Vec<String>> {
        let tags: spfs::Result<Vec<_>> = self.inner.iter_tags().collect();
        let tags = tags?
            .into_iter()
            .map(|(spec, _)| spec.to_string())
            .collect();
        Ok(tags)
    }

    pub fn ls_tags(&self, base: &str) -> Result<Vec<String>> {
        let path = relative_path::RelativePath::new(base);
        let tags: Vec<_> = self.inner.ls_tags(&path)?.collect();
        Ok(tags)
    }

    pub fn remove_tag_stream(&mut self, tag: &str) -> Result<()> {
        let tag = tag.parse()?;
        self.inner.remove_tag_stream(&tag)?;
        Ok(())
    }

    pub fn write_spec(&mut self, tag: &str, payload: Vec<u8>) -> Result<()> {
        let tag = tag.parse()?;
        let (digest, size) = self.inner.write_data(Box::new(&mut payload.as_slice()))?;
        let blob = spfs::graph::Blob {
            payload: digest.clone(),
            size: size,
        };
        self.inner.write_blob(blob)?;
        self.inner.push_tag(&tag, &digest)?;
        Ok(())
    }

    pub fn read_spec(&self, digest: &Digest) -> Result<String> {
        let mut buf = Vec::new();
        let mut payload = self.inner.open_payload(&digest.inner)?;
        std::io::copy(&mut payload, &mut buf)?;
        Ok(unsafe { String::from_utf8_unchecked(buf) })
    }

    pub fn flush(&mut self) -> Result<()> {
        match &mut self.inner {
            spfs::storage::RepositoryHandle::Tar(tar) => Ok(tar.flush()?),
            _ => Ok(()),
        }
    }
}

/// Return the local packages repository used for development.
pub fn local_repository() -> Result<SpFSRepository> {
    let config = spfs::load_config()?;
    let repo = config.get_repository()?;
    Ok(SpFSRepository { inner: repo.into() })
}

/// Return the remote repository of the given name.
///
/// If not name is specified, return the default spfs repository.
pub fn remote_repository<S: AsRef<str>>(name: S) -> Result<SpFSRepository> {
    let config = spfs::load_config()?;
    let repo = config.get_remote(name)?;
    Ok(SpFSRepository { inner: repo })
}
