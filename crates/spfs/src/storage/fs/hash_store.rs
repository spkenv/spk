// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::task::Poll;

use futures::{Future, Stream};
use tokio::io::AsyncWriteExt;

use crate::runtime::makedirs_with_perms;
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./hash_store_test.rs"]
mod hash_store_test;

static WORK_DIRNAME: &str = "work";

pub(crate) enum PersistableObject {
    #[cfg(test)]
    EmptyFile,
    WorkingFile {
        working_file: PathBuf,
        copied: u64,
    },
}

pub struct FSHashStore {
    root: PathBuf,
    /// permissions used when creating new directories
    pub directory_permissions: u32,
    /// permissions used when creating new files
    pub file_permissions: u32,
}

impl FSHashStore {
    pub fn open<P: AsRef<Path>>(root: P) -> Result<Self> {
        Ok(Self::open_unchecked(root.as_ref().canonicalize()?))
    }

    pub fn open_unchecked<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            directory_permissions: 0o777, // this is a shared store for all users
            file_permissions: 0o666,      // read+write is required to make hard links
        }
    }

    /// Return the root directory of this storage.
    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }

    /// The folder in which in-progress data is stored temporarily
    pub fn workdir(&self) -> PathBuf {
        self.root.join(&WORK_DIRNAME)
    }

    pub fn find(&self, search_criteria: crate::graph::DigestSearchCriteria) -> FSHashStoreIter {
        FSHashStoreIter::with_criteria(&self.root(), search_criteria)
    }

    pub fn iter(&self) -> FSHashStoreIter {
        FSHashStoreIter::new(&self.root())
    }

    /// Return true if the given digest is stored in this storage
    ///
    /// Upon error, false is returned
    pub fn has_digest(&self, digest: &encoding::Digest) -> bool {
        let path = self.build_digest_path(digest);
        path.exists()
    }

    /// Write all data in the given reader to a file in this storage
    pub async fn write_data(
        &self,
        mut reader: Pin<Box<dyn tokio::io::AsyncRead + Send + Sync + 'static>>,
    ) -> Result<(encoding::Digest, u64)> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_file = self.workdir().join(uuid);

        self.ensure_base_dir(&working_file)?;
        let mut writer = tokio::io::BufWriter::new(
            tokio::fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&working_file)
                .await?,
        );
        let mut hasher = encoding::Hasher::with_target(&mut writer);
        let copied = match tokio::io::copy(&mut reader, &mut hasher).await {
            Err(err) => {
                let _ = tokio::fs::remove_file(working_file).await;
                return Err(Error::wrap_io(err, "Failed to write object data"));
            }
            Ok(s) => s,
        };

        let digest = hasher.digest();
        if let Err(err) = writer.flush().await {
            return Err(Error::wrap_io(err, "Failed to finalize object write"));
        }
        if let Err(err) = writer.get_ref().sync_all().await {
            return Err(Error::wrap_io(err, "Failed to sync object write"));
        }

        self.persist_object_with_digest(
            PersistableObject::WorkingFile {
                working_file,
                copied,
            },
            digest,
        )
        .await
    }

    pub(crate) async fn persist_object_with_digest(
        &self,
        persistable_object: PersistableObject,
        digest: encoding::Digest,
    ) -> Result<(encoding::Digest, u64)> {
        let path = self.build_digest_path(&digest);
        self.ensure_base_dir(&path)?;

        let copied = match persistable_object {
            #[cfg(test)]
            PersistableObject::EmptyFile => {
                tokio::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)
                    .await?;
                0
            }
            PersistableObject::WorkingFile {
                working_file,
                copied,
            } => {
                if let Err(err) = tokio::fs::rename(&working_file, &path).await {
                    let _ = tokio::fs::remove_file(working_file).await;
                    match err.kind() {
                        ErrorKind::AlreadyExists => (),
                        _ => return Err(Error::wrap_io(err, "Failed to store object")),
                    }
                }
                copied
            }
        };

        if let Err(_err) = tokio::fs::set_permissions(
            &path,
            std::fs::Permissions::from_mode(self.file_permissions),
        )
        .await
        {
            // not a good enough reason to fail entirely
            #[cfg(feature = "sentry")]
            sentry::capture_event(sentry::protocol::Event {
                message: Some(format!("{:?}", _err)),
                level: sentry::protocol::Level::Warning,
                ..Default::default()
            });
        }

        Ok((digest, copied))
    }

    pub fn build_digest_path(&self, digest: &encoding::Digest) -> PathBuf {
        let digest_str = digest.to_string();
        self.root.join(&digest_str[..2]).join(&digest_str[2..])
    }

    pub fn ensure_base_dir<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        match path.as_ref().parent() {
            Some(parent) => makedirs_with_perms(parent, self.directory_permissions),
            _ => Ok(()),
        }
    }

    /// Given a valid storage path, get the object digest.
    ///
    /// This method does not validate the path and will provide
    /// invalid references if given an invalid path.
    pub async fn get_digest_from_path<P: AsRef<Path>>(&self, path: P) -> Result<encoding::Digest> {
        use std::path::Component::Normal;
        let path = tokio::fs::canonicalize(path).await?;
        let mut parts: Vec<_> = path.components().collect();
        let last = parts.pop();
        let second_last = parts.pop();
        match (second_last, last) {
            (Some(Normal(a)), Some(Normal(b))) => {
                encoding::parse_digest(a.to_string_lossy() + b.to_string_lossy())
            }
            _ => Err(format!("not a valid digest path: {:?}", &path).into()),
        }
    }

    /// Given a shortened digest, resolve the full object path.
    ///
    /// # Errors:
    /// - spfs::Error::UnknownObject: if the digest cannot be resolved
    /// - spfs::Error::AmbiguousReference: if the digest resolves to more than one path
    pub async fn resolve_full_digest_path(
        &self,
        partial: &encoding::PartialDigest,
    ) -> Result<PathBuf> {
        let short_digest = partial.to_string();
        let (dirname, file_prefix) = (&short_digest[..2], &short_digest[2..]);
        let dirpath = self.root.join(dirname);
        if short_digest.len() == encoding::DIGEST_SIZE {
            return Ok(dirpath.join(file_prefix));
        }

        let entries: Vec<std::ffi::OsString> = match tokio::fs::read_dir(&dirpath).await {
            Err(err) => {
                return match err.kind() {
                    ErrorKind::NotFound => Err(Error::UnknownReference(short_digest)),
                    _ => Err(err.into()),
                }
            }
            Ok(mut read_dir) => {
                let mut mapped = Vec::new();
                while let Some(next) = read_dir.next_entry().await? {
                    mapped.push(next.file_name());
                }
                mapped
            }
        };

        let options: Vec<std::ffi::OsString> = entries
            .into_iter()
            .filter(|x| x.to_string_lossy().starts_with(file_prefix))
            .collect();
        match options.len() {
            0 => Err(Error::UnknownReference(short_digest)),
            1 => Ok(dirpath.join(options.get(0).unwrap())),
            _ => Err(Error::AmbiguousReference(short_digest)),
        }
    }

    /// Return the shortened version of the given digest.
    ///
    /// This implementation improves greatly on the base one by limiting
    /// the possible conflicts to a subdirectory (and subset of all digests)
    pub async fn get_shortened_digest(&self, digest: &encoding::Digest) -> Result<String> {
        let filepath = self.build_digest_path(digest);
        let entries: Vec<_> = match tokio::fs::read_dir(filepath.parent().unwrap()).await {
            Err(err) => {
                return match err.kind() {
                    ErrorKind::NotFound => Err(Error::UnknownObject(*digest)),
                    _ => Err(err.into()),
                };
            }
            Ok(mut read_dir) => {
                let mut mapped = Vec::new();
                while let Some(next) = read_dir.next_entry().await? {
                    mapped.push(next.file_name().to_string_lossy().to_string());
                }
                mapped
            }
        };

        let digest_str = digest.to_string();
        let mut shortest_size = 8;
        let mut shortest = &digest_str[2..shortest_size];
        for other in entries {
            if &other[0..shortest_size] != shortest || other == digest_str[2..] {
                continue;
            }
            while &other[0..shortest_size] == shortest {
                shortest_size += 8;
                shortest = &digest_str[2..shortest_size];
            }
        }
        Ok(digest_str[..shortest_size].to_string())
    }

    /// Resolve the complete object digest from a shortened one.
    ///
    /// # Errors:
    /// - spfs::Error::UnknownObject: if the digest cannot be resolved
    /// - spfs::Error::AmbiguousReference: if the digest resolves to more than one path
    pub async fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
    ) -> Result<encoding::Digest> {
        if let Some(complete) = partial.to_digest() {
            return Ok(complete);
        }
        let path = self.resolve_full_digest_path(partial).await?;
        self.get_digest_from_path(path).await
    }
}

enum FSHashStoreIterState {
    OpeningRoot {
        future: Pin<Box<dyn Future<Output = std::io::Result<tokio::fs::ReadDir>> + Send>>,
    },
    AwaitingSubdir {
        root: tokio::fs::ReadDir,
    },
    OpeningSubdir {
        name: std::ffi::OsString,
        root: tokio::fs::ReadDir,
        future: Pin<Box<dyn Future<Output = std::io::Result<tokio::fs::ReadDir>> + Send>>,
    },
    IteratingSubdir {
        name: std::ffi::OsString,
        root: tokio::fs::ReadDir,
        read_dir: tokio::fs::ReadDir,
    },
}

impl std::fmt::Debug for FSHashStoreIterState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpeningRoot { .. } => f.debug_struct("OpeningRoot").finish(),
            Self::AwaitingSubdir { .. } => f.debug_struct("AwaitingSubdir").finish(),
            Self::OpeningSubdir { .. } => f.debug_struct("OpeningSubdir").finish(),
            Self::IteratingSubdir { .. } => f.debug_struct("IteratingSubdir").finish(),
        }
    }
}

pub struct FSHashStoreIter {
    root: PathBuf,
    state: Option<FSHashStoreIterState>,
    criteria: crate::graph::DigestSearchCriteria,
}

impl FSHashStoreIter {
    pub fn new<P: Into<PathBuf>>(root: P) -> Self {
        Self::with_criteria(root, crate::graph::DigestSearchCriteria::All)
    }

    pub fn with_criteria<P: Into<PathBuf>>(
        root: P,
        criteria: crate::graph::DigestSearchCriteria,
    ) -> Self {
        let root = root.into();
        let state = Some(FSHashStoreIterState::OpeningRoot {
            future: Box::pin(tokio::fs::read_dir(root.clone())),
        });
        Self {
            root,
            state,
            criteria,
        }
    }
}

impl Stream for FSHashStoreIter {
    type Item = Result<encoding::Digest>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        use FSHashStoreIterState::*;
        match self.state.take() {
            Some(OpeningRoot { mut future }) => match Pin::new(&mut future).poll(cx) {
                Poll::Pending => {
                    self.state = Some(OpeningRoot { future });
                    Poll::Pending
                }
                Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err.into()))),
                Poll::Ready(Ok(root)) => {
                    self.state = Some(AwaitingSubdir { root });
                    self.poll_next(cx)
                }
            },
            Some(AwaitingSubdir { mut root }) => match root.poll_next_entry(cx) {
                Poll::Pending => {
                    self.state = Some(AwaitingSubdir { root });
                    Poll::Pending
                }
                Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err.into()))),
                Poll::Ready(Ok(Some(next_dir))) => {
                    let name = next_dir.file_name();
                    if name.as_os_str() == OsStr::new(WORK_DIRNAME) {
                        self.state = Some(AwaitingSubdir { root });
                        return self.poll_next(cx);
                    }
                    let mut process_subdir = |mut this: Pin<&mut Self>, root, name| {
                        let path = this.root.join(&name);
                        let future = Box::pin(tokio::fs::read_dir(path));
                        this.state = Some(OpeningSubdir { root, name, future });
                        this.poll_next(cx)
                    };
                    match &self.criteria {
                        crate::graph::DigestSearchCriteria::All => process_subdir(self, root, name),
                        crate::graph::DigestSearchCriteria::StartsWith(bytes)
                            if (name.len() < bytes.len() && bytes.starts_with(name.as_bytes()))
                                || (name.len() >= bytes.len()
                                    && name.as_bytes().starts_with(bytes)) =>
                        {
                            process_subdir(self, root, name)
                        }
                        crate::graph::DigestSearchCriteria::StartsWith(_) => {
                            // Keep looking for a subdirectory that matches the search criteria
                            self.state = Some(AwaitingSubdir { root });
                            self.poll_next(cx)
                        }
                    }
                }
                Poll::Ready(Ok(None)) => Poll::Ready(None),
            },
            Some(OpeningSubdir {
                root,
                name,
                mut future,
            }) => match Pin::new(&mut future).poll(cx) {
                Poll::Pending => {
                    self.state = Some(OpeningSubdir { root, name, future });
                    Poll::Pending
                }
                Poll::Ready(Err(err)) => match err.raw_os_error() {
                    Some(libc::ENOTDIR) => {
                        tracing::debug!(?name, "found non-directory in hash storage");
                        self.state = Some(AwaitingSubdir { root });
                        self.poll_next(cx)
                    }
                    _ => Poll::Ready(Some(Err(err.into()))),
                },
                Poll::Ready(Ok(read_dir)) => {
                    self.state = Some(IteratingSubdir {
                        root,
                        name,
                        read_dir,
                    });
                    self.poll_next(cx)
                }
            },
            Some(IteratingSubdir {
                root,
                name,
                mut read_dir,
            }) => match read_dir.poll_next_entry(cx) {
                Poll::Pending => {
                    self.state = Some(IteratingSubdir {
                        root,
                        name,
                        read_dir,
                    });
                    Poll::Pending
                }
                Poll::Ready(Err(err)) => Poll::Ready(Some(Err(err.into()))),
                Poll::Ready(Ok(None)) => match &self.criteria {
                    crate::graph::DigestSearchCriteria::All => {
                        self.state = Some(AwaitingSubdir { root });
                        self.poll_next(cx)
                    }
                    crate::graph::DigestSearchCriteria::StartsWith(bytes)
                        if bytes.starts_with(name.as_bytes()) =>
                    {
                        // After reading the contents of the subdirectory that matches
                        // the search criteria, there are no more results.
                        Poll::Ready(None)
                    }
                    crate::graph::DigestSearchCriteria::StartsWith(_) => {
                        self.state = Some(AwaitingSubdir { root });
                        self.poll_next(cx)
                    }
                },
                Poll::Ready(Ok(Some(entry))) => {
                    let mut digest_str = name.to_string_lossy().to_string();
                    digest_str.push_str(entry.file_name().to_string_lossy().as_ref());
                    self.state = Some(IteratingSubdir {
                        root,
                        name,
                        read_dir,
                    });
                    match encoding::parse_digest(&digest_str) {
                        Ok(digest) => match &self.criteria {
                            crate::graph::DigestSearchCriteria::All => {
                                Poll::Ready(Some(Ok(digest)))
                            }
                            crate::graph::DigestSearchCriteria::StartsWith(bytes)
                                if digest_str.as_bytes().starts_with(bytes.as_slice()) =>
                            {
                                Poll::Ready(Some(Ok(digest)))
                            }
                            crate::graph::DigestSearchCriteria::StartsWith(_) => self.poll_next(cx),
                        },
                        Err(err) => {
                            tracing::debug!(
                                ?err, name = ?digest_str,
                                "invalid digest in file storage",
                            );
                            self.poll_next(cx)
                        }
                    }
                }
            },
            None => Poll::Ready(None),
        }
    }
}
