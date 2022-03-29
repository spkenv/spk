// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::Stream;
use relative_path::RelativePath;
use tar::{Archive, Builder};

use crate::graph;
use crate::storage::{tag::TagSpecAndTagStream, EntryType};
use crate::Result;
use crate::{encoding, prelude::*, storage, tracking};

/// Configuration for a tar repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub path: std::path::PathBuf,
}

#[async_trait::async_trait]
impl storage::FromUrl for Config {
    async fn from_url(url: &url::Url) -> Result<Self> {
        Ok(Self {
            path: std::path::PathBuf::from(url.path()),
        })
    }
}

/// An spfs repository in a tarball.
///
/// Tarball repos are unpacked to a temporary directory on creation
/// and re-packed to an archive on drop. This is not efficient for
/// large repos and is not safe for multiple reader/writers.
pub struct TarRepository {
    up_to_date: AtomicBool,
    archive: std::path::PathBuf,
    repo_dir: tempdir::TempDir,
    repo: crate::storage::fs::FSRepository,
}

#[async_trait::async_trait]
impl storage::FromConfig for TarRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> Result<Self> {
        Self::create(&config.path).await
    }
}

impl std::fmt::Debug for TarRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TarRepository<{:?}>", &self.archive))
    }
}

impl TarRepository {
    pub async fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
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
        Self::open(path).await
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().canonicalize()?;
        let mut file = std::fs::File::open(&path)?;
        let mut archive = Archive::new(&mut file);
        let tmpdir = tempdir::TempDir::new("spfs-tar-repo")?;
        let repo_path = tmpdir.path().to_path_buf();
        archive.unpack(&repo_path)?;
        Ok(Self {
            up_to_date: AtomicBool::new(false),
            archive: path,
            repo_dir: tmpdir,
            repo: crate::storage::fs::FSRepository::create(&repo_path).await?,
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
        self.up_to_date
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

impl Drop for TarRepository {
    fn drop(&mut self) {
        if self.up_to_date.load(Ordering::Acquire) {
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

#[async_trait::async_trait]
impl graph::DatabaseView for TarRepository {
    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        self.repo.read_object(digest).await
    }

    fn iter_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.repo.iter_digests()
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        self.repo.iter_objects()
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        self.repo.walk_objects(root)
    }
}

#[async_trait::async_trait]
impl graph::Database for TarRepository {
    async fn write_object(&self, obj: &graph::Object) -> Result<()> {
        self.repo.write_object(obj).await?;
        self.up_to_date.store(false, Ordering::Release);
        Ok(())
    }

    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        self.repo.remove_object(digest).await?;
        self.up_to_date.store(false, Ordering::Release);
        Ok(())
    }
}

#[async_trait::async_trait]
impl PayloadStorage for TarRepository {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.repo.iter_payload_digests()
    }

    async fn write_data(
        &self,
        reader: Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>,
    ) -> Result<(encoding::Digest, u64)> {
        let res = self.repo.write_data(reader).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(res)
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>> {
        self.repo.open_payload(digest).await
    }

    async fn remove_payload(&self, digest: encoding::Digest) -> Result<()> {
        self.repo.remove_payload(digest).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

#[async_trait::async_trait]
impl TagStorage for TarRepository {
    async fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        self.repo.resolve_tag(tag_spec).await
    }

    fn ls_tags(
        &self,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        self.repo.ls_tags(path)
    }

    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        self.repo.find_tags(digest)
    }

    fn iter_tag_streams(&self) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        self.repo.iter_tag_streams()
    }

    async fn read_tag(
        &self,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        self.repo.read_tag(tag).await
    }

    async fn push_raw_tag(&self, tag: &tracking::Tag) -> Result<()> {
        self.repo.push_raw_tag(tag).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    async fn remove_tag_stream(&self, tag: &tracking::TagSpec) -> Result<()> {
        self.repo.remove_tag_stream(tag).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    async fn remove_tag(&self, tag: &tracking::Tag) -> Result<()> {
        self.repo.remove_tag(tag).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
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
