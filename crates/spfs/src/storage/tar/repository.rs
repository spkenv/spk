// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;
use std::io::BufReader;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use futures::Stream;
use relative_path::RelativePath;
use tar::{Archive, Builder};

use crate::config::{ToAddress, pathbuf_deserialize_with_tilde_expansion};
use crate::graph::ObjectProto;
use crate::prelude::*;
use crate::storage::fs::DURABLE_EDITS_DIR;
use crate::storage::tag::TagSpecAndTagStream;
use crate::storage::{
    EntryType,
    OpenRepositoryError,
    OpenRepositoryResult,
    TagNamespace,
    TagNamespaceBuf,
    TagStorageMut,
};
use crate::tracking::BlobRead;
use crate::{Error, Result, encoding, graph, storage, tracking};

/// Configuration for a tar repository
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    #[serde(deserialize_with = "pathbuf_deserialize_with_tilde_expansion::deserialize")]
    pub path: std::path::PathBuf,
}

impl ToAddress for Config {
    fn to_address(&self) -> Result<url::Url> {
        url::Url::from_file_path(&self.path).map_err(|err| {
            crate::Error::String(format!(
                "Tar file path does not create a valid url: {err:?}"
            ))
        })
    }
}

#[async_trait::async_trait]
impl storage::FromUrl for Config {
    async fn from_url(url: &url::Url) -> OpenRepositoryResult<Self> {
        // Use to_file_path() to properly decode percent-encoded characters
        // (e.g., %20 -> space) in the URL path. This also handles Windows
        // drive letters correctly (e.g., file:///C:/path -> C:\path).
        let path = url.to_file_path().map_err(|_| {
            OpenRepositoryError::UnsupportedRepositoryType(format!("Invalid file URL: {}", url))
        })?;
        Ok(Self { path })
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
    repo_dir: tempfile::TempDir,
    repo: crate::storage::fs::MaybeOpenFsRepository,
}

#[async_trait::async_trait]
impl storage::FromConfig for TarRepository {
    type Config = Config;

    async fn from_config(config: Self::Config) -> OpenRepositoryResult<Self> {
        Self::create(&config.path).await
    }
}

impl std::fmt::Debug for TarRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("TarRepository<{:?}>", &self.archive))
    }
}

impl TarRepository {
    pub async fn create<P: AsRef<Path>>(path: P) -> OpenRepositoryResult<Self> {
        let path = path.as_ref();
        if !path.exists() {
            if let Some(parent) = path.parent() {
                crate::runtime::makedirs_with_perms(parent, 0o777)
                    .map_err(|_| OpenRepositoryError::CouldNotCreateTarParent(parent.to_owned()))?;
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|source| OpenRepositoryError::FailedToOpenArchive {
                    path: path.to_owned(),
                    source,
                })?;
            Builder::new(&mut file).finish().map_err(|source| {
                OpenRepositoryError::FailedToCloseArchive {
                    path: path.to_owned(),
                    source,
                }
            })?;
        }
        Self::open(path).await
    }

    /// Remove the top-level durable directory, assuming it is empty.
    /// This is used when exporting packages from another repo via a
    /// tar repo, and you do not want to include any durable runtime
    /// related data.
    pub async fn remove_durable_dir(&self) -> Result<()> {
        let path = self.repo_dir.path().join(DURABLE_EDITS_DIR);
        // This will fail if the durable edits directory for runtimes has something in it.
        tokio::fs::remove_dir(&path)
            .await
            .map_err(|err| Error::RuntimeWriteError(path, err))
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub async fn open<P: AsRef<Path>>(path: P) -> OpenRepositoryResult<Self> {
        let path = dunce::canonicalize(&path).map_err(|source| {
            OpenRepositoryError::FailedToOpenArchive {
                path: path.as_ref().to_owned(),
                source,
            }
        })?;
        let mut file = BufReader::new(std::fs::File::open(&path).map_err(|source| {
            OpenRepositoryError::FailedToOpenArchive {
                path: path.clone(),
                source,
            }
        })?);
        let mut archive = Archive::new(&mut file);
        let tmpdir = tempfile::Builder::new()
            .prefix("spfs-tar-repo")
            .tempdir()
            .map_err(|source| OpenRepositoryError::FailedToUnpackArchive {
                path: "<new temporary directory>".into(),
                source,
            })?;
        let repo_path = tmpdir.path().to_path_buf();
        archive.unpack(&repo_path).map_err(|source| {
            OpenRepositoryError::FailedToUnpackArchive {
                path: repo_path.clone(),
                source,
            }
        })?;
        Ok(Self {
            up_to_date: AtomicBool::new(false),
            archive: path,
            repo_dir: tmpdir,
            repo: crate::storage::fs::MaybeOpenFsRepository::create(&repo_path).await?,
        })
    }

    pub fn flush(&self) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.archive)
            .map_err(|err| {
                Error::StorageWriteError(
                    "open tar repository for write and truncate",
                    self.archive.clone(),
                    err,
                )
            })?;
        let mut builder = Builder::new(&mut file);
        builder
            .append_dir_all(".", self.repo_dir.path())
            .map_err(|err| {
                Error::StorageWriteError(
                    "append_all_dir on tar repository builder",
                    self.archive.clone(),
                    err,
                )
            })?;
        builder.finish().map_err(|err| {
            Error::StorageWriteError(
                "finish on tar repository builder in flush",
                self.archive.clone(),
                err,
            )
        })?;
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
                panic!("failed to flush tar repository, archive may be corrupt: {err:?}");
            }
        }
    }
}

#[async_trait::async_trait]
impl graph::DatabaseView for TarRepository {
    async fn has_object(&self, digest: encoding::Digest) -> bool {
        self.repo.has_object(digest).await
    }

    async fn read_object(&self, digest: encoding::Digest) -> Result<graph::Object> {
        self.repo.read_object(digest).await
    }

    fn find_digests(
        &self,
        search_criteria: graph::DigestSearchCriteria,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.repo.find_digests(search_criteria)
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        self.repo.iter_objects()
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        self.repo.walk_objects(root)
    }

    async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
    ) -> Result<encoding::Digest> {
        self.repo.resolve_full_digest(partial).await
    }
}

#[async_trait::async_trait]
impl graph::Database for TarRepository {
    async fn remove_object(&self, digest: encoding::Digest) -> Result<()> {
        self.repo.remove_object(digest).await?;
        self.up_to_date.store(false, Ordering::Release);
        Ok(())
    }

    async fn remove_object_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        let deleted = self
            .repo
            .remove_object_if_older_than(older_than, digest)
            .await?;
        if deleted {
            self.up_to_date.store(false, Ordering::Release);
        }
        Ok(deleted)
    }
}

#[async_trait::async_trait]
impl graph::DatabaseExt for TarRepository {
    async fn write_object<T: ObjectProto>(&self, obj: &graph::FlatObject<T>) -> Result<()> {
        self.repo.write_object(obj).await?;
        self.up_to_date.store(false, Ordering::Release);
        Ok(())
    }
}

#[async_trait::async_trait]
impl PayloadStorage for TarRepository {
    async fn has_payload(&self, digest: encoding::Digest) -> bool {
        self.repo.has_payload(digest).await
    }

    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send>> {
        self.repo.iter_payload_digests()
    }

    async unsafe fn write_data(
        &self,
        reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        // Safety: we are wrapping the same underlying unsafe function and
        // so the same safety holds for our callers
        let res = unsafe { self.repo.write_data(reader).await? };
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(res)
    }

    async fn open_payload(
        &self,
        digest: encoding::Digest,
    ) -> Result<(Pin<Box<dyn BlobRead>>, std::path::PathBuf)> {
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
    #[inline]
    fn get_tag_namespace(&self) -> Option<Cow<'_, TagNamespace>> {
        self.repo.get_tag_namespace()
    }

    async fn resolve_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<tracking::Tag> {
        self.repo.resolve_tag_in_namespace(namespace, tag).await
    }

    fn ls_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        path: &RelativePath,
    ) -> Pin<Box<dyn Stream<Item = Result<EntryType>> + Send>> {
        self.repo.ls_tags_in_namespace(namespace, path)
    }

    fn find_tags_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        digest: &encoding::Digest,
    ) -> Pin<Box<dyn Stream<Item = Result<tracking::TagSpec>> + Send>> {
        self.repo.find_tags_in_namespace(namespace, digest)
    }

    fn iter_tag_streams_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
    ) -> Pin<Box<dyn Stream<Item = Result<TagSpecAndTagStream>> + Send>> {
        self.repo.iter_tag_streams_in_namespace(namespace)
    }

    async fn read_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<tracking::Tag>> + Send>>> {
        self.repo.read_tag_in_namespace(namespace, tag).await
    }

    async fn insert_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        self.repo.insert_tag_in_namespace(namespace, tag).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    async fn remove_tag_stream_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::TagSpec,
    ) -> Result<()> {
        self.repo
            .remove_tag_stream_in_namespace(namespace, tag)
            .await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    async fn remove_tag_in_namespace(
        &self,
        namespace: Option<&TagNamespace>,
        tag: &tracking::Tag,
    ) -> Result<()> {
        self.repo.remove_tag_in_namespace(namespace, tag).await?;
        self.up_to_date
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

impl TagStorageMut for TarRepository {
    fn try_set_tag_namespace(
        &mut self,
        tag_namespace: Option<TagNamespaceBuf>,
    ) -> Result<Option<TagNamespaceBuf>> {
        self.repo.try_set_tag_namespace(tag_namespace)
    }
}

impl Address for TarRepository {
    fn address(&self) -> Cow<'_, url::Url> {
        Cow::Owned(
            url::Url::from_file_path(&self.repo_dir).expect("unexpected failure creating url"),
        )
    }
}
