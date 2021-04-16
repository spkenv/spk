// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;

use tar::{Archive, Builder};

use crate::graph;
use crate::Result;
use crate::{encoding, prelude::*, tracking};

/// An spfs repository in a tarball.
///
/// Tarball repos are unpacked to a temporary directory on creation
/// and re-packed to an archive on drop. This is not efficient for
/// large repos and is not safe for multiple reader/writers.
pub struct TarRepository {
    up_to_date: bool,
    archive: std::path::PathBuf,
    repo_dir: tempdir::TempDir,
    repo: crate::storage::fs::FSRepository,
}

impl std::fmt::Debug for TarRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TarRepository<{:?}>", &self.archive))
    }
}

impl TarRepository {
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                crate::runtime::makedirs_with_perms(parent, 0o777)?;
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            Builder::new(&mut file).finish()?;
        }
        Self::open(path)
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().canonicalize()?;
        let mut file = std::fs::File::open(&path)?;
        let mut archive = Archive::new(&mut file);
        let tmpdir = tempdir::TempDir::new("spfs-tar-repo")?;
        let repo_path = tmpdir.path().to_path_buf();
        archive.unpack(&repo_path)?;
        Ok(Self {
            up_to_date: false,
            archive: path,
            repo_dir: tmpdir,
            repo: crate::storage::fs::FSRepository::create(&repo_path)?,
        })
    }

    pub fn flush(&mut self) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.archive)?;
        let mut builder = Builder::new(&mut file);
        builder.append_dir_all(".", self.repo_dir.path())?;
        builder.finish()?;
        self.up_to_date = true;
        Ok(())
    }
}

impl Drop for TarRepository {
    fn drop(&mut self) {
        if self.up_to_date {
            return;
        }
        if let Err(err) = self.flush() {
            tracing::error!(
                ?err,
                "failed to flush tar repository, archive may be corrupt"
            );
            #[cfg(test)]
            {
                panic!(
                    "failed to flush tar repository, archive may be corrupt: {:?}",
                    err
                );
            }
        }
    }
}

impl graph::DatabaseView for TarRepository {
    fn read_object(&self, digest: &encoding::Digest) -> graph::Result<graph::Object> {
        self.repo.read_object(digest)
    }

    fn iter_digests(&self) -> Box<dyn Iterator<Item = graph::Result<encoding::Digest>>> {
        self.repo.iter_digests()
    }

    fn iter_objects<'db>(&'db self) -> graph::DatabaseIterator<'db> {
        self.repo.iter_objects()
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        self.repo.walk_objects(root)
    }
}

impl graph::Database for TarRepository {
    fn write_object(&mut self, obj: &graph::Object) -> graph::Result<()> {
        self.repo.write_object(obj)?;
        self.up_to_date = false;
        Ok(())
    }

    fn remove_object(&mut self, digest: &encoding::Digest) -> graph::Result<()> {
        self.repo.remove_object(digest)?;
        self.up_to_date = false;
        Ok(())
    }
}

impl PayloadStorage for TarRepository {
    fn iter_payload_digests(&self) -> Box<dyn Iterator<Item = Result<encoding::Digest>>> {
        self.repo.iter_payload_digests()
    }

    fn write_data(
        &mut self,
        reader: Box<&mut dyn std::io::Read>,
    ) -> Result<(encoding::Digest, u64)> {
        let res = self.repo.write_data(reader)?;
        self.up_to_date = false;
        Ok(res)
    }

    fn open_payload(&self, digest: &encoding::Digest) -> Result<Box<dyn std::io::Read>> {
        self.repo.open_payload(digest)
    }

    fn remove_payload(&mut self, digest: &encoding::Digest) -> Result<()> {
        self.repo.remove_payload(digest)?;
        self.up_to_date = false;
        Ok(())
    }
}

impl TagStorage for TarRepository {
    fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        self.repo.resolve_tag(tag_spec)
    }

    fn ls_tags(
        &self,
        path: &relative_path::RelativePath,
    ) -> Result<Box<dyn Iterator<Item = String>>> {
        self.repo.ls_tags(path)
    }

    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Box<dyn Iterator<Item = Result<tracking::TagSpec>>> {
        self.repo.find_tags(digest)
    }

    fn iter_tag_streams(
        &self,
    ) -> Box<
        dyn Iterator<Item = Result<(tracking::TagSpec, Box<dyn Iterator<Item = tracking::Tag>>)>>,
    > {
        self.repo.iter_tag_streams()
    }

    fn read_tag(&self, tag: &tracking::TagSpec) -> Result<Box<dyn Iterator<Item = tracking::Tag>>> {
        self.repo.read_tag(tag)
    }

    fn push_raw_tag(&mut self, tag: &tracking::Tag) -> Result<()> {
        self.repo.push_raw_tag(tag)?;
        self.up_to_date = false;
        Ok(())
    }

    fn remove_tag_stream(&mut self, tag: &tracking::TagSpec) -> Result<()> {
        self.repo.remove_tag_stream(tag)?;
        self.up_to_date = false;
        Ok(())
    }

    fn remove_tag(&mut self, tag: &tracking::Tag) -> Result<()> {
        self.repo.remove_tag(tag)?;
        self.up_to_date = false;
        Ok(())
    }
}

impl BlobStorage for TarRepository {}
impl ManifestStorage for TarRepository {}
impl LayerStorage for TarRepository {}
impl PlatformStorage for TarRepository {}
impl Repository for TarRepository {
    fn address(&self) -> url::Url {
        url::Url::from_file_path(&self.repo_dir).expect("unexpected failure creating url")
    }
}
