// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::io::ErrorKind;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_stream::try_stream;
use close_err::Closable;
use futures::{Stream, TryStreamExt};
use tokio::fs::DirEntry;
use tokio::io::AsyncWriteExt;

use crate::runtime::makedirs_with_perms;
use crate::storage::{OpenRepositoryError, OpenRepositoryResult};
use crate::tracking::BlobRead;
use crate::{Error, OsError, Result, encoding};

#[cfg(test)]
#[path = "./hash_store_test.rs"]
mod hash_store_test;

pub(crate) const PROXY_DIRNAME: &str = "proxy";
const WORK_DIRNAME: &str = "work";

pub(crate) enum PersistableObject {
    #[cfg(test)]
    EmptyFile,
    WorkingFile {
        working_file: PathBuf,
        copied: u64,
        object_permissions: Option<u32>,
    },
}

pub struct FsHashStore {
    root: PathBuf,
    /// permissions used when creating new directories
    pub directory_permissions: u32,
    /// permissions used when creating new files
    pub file_permissions: u32,
}

impl FsHashStore {
    pub fn open<P: AsRef<Path>>(root: P) -> OpenRepositoryResult<Self> {
        let root = root.as_ref();
        Ok(Self::open_unchecked(dunce::canonicalize(root).map_err(
            |source| OpenRepositoryError::PathNotInitialized {
                path: root.to_owned(),
                source,
            },
        )?))
    }

    pub fn open_unchecked<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            directory_permissions: 0o777, // this is a shared store for all users
            file_permissions: 0o666,      // read+write is required to make hard links
        }
    }

    /// The folder where payloads are copied to have the expected ownership
    /// and permissions suitable for hard-linking into a render.
    pub fn proxydir(&self) -> PathBuf {
        self.root.join(PROXY_DIRNAME)
    }

    /// Return the root directory of this storage.
    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }

    /// The folder in which in-progress data is stored temporarily
    pub fn workdir(&self) -> PathBuf {
        self.root.join(WORK_DIRNAME)
    }

    async fn find_in_entry(
        search_criteria: crate::graph::DigestSearchCriteria,
        entry: DirEntry,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send + Sync + 'static>> {
        let entry_filename = entry.file_name();
        let entry_filename = entry_filename.to_string_lossy().into_owned();
        if entry_filename == WORK_DIRNAME || entry_filename == PROXY_DIRNAME {
            return Box::pin(futures::stream::empty());
        }

        let entry_partial = match encoding::PartialDigest::parse(&entry_filename) {
            Err(err) => {
                tracing::debug!(?err, "invalid digest directory in file storage",);
                return Box::pin(futures::stream::empty());
            }
            Ok(partial) => partial,
        };
        let entry_bytes = entry_partial.as_slice();

        match &search_criteria {
            crate::graph::DigestSearchCriteria::StartsWith(bytes)
                // If the directory name is shorter than the prefix, check that
                // the prefix starts with the directory name.
                if entry_bytes.len() < bytes.len() && !bytes.starts_with(entry_bytes) => {
                    return Box::pin(futures::stream::empty());
                }
            crate::graph::DigestSearchCriteria::StartsWith(bytes)
                // If the directory name is longer than the prefix, check that
                // the directory name starts with the prefix.
                if entry_bytes.len() >= bytes.len() && !entry_bytes.starts_with(bytes) => {
                    return Box::pin(futures::stream::empty());
                }
            _ => {}
        };

        let mut subdir = match tokio::fs::read_dir(entry.path()).await {
            Err(err) => match err.os_error() {
                Some(libc::ENOTDIR) => {
                    tracing::debug!(?entry_filename, "found non-directory in hash storage");
                    return Box::pin(futures::stream::empty());
                }
                _ => {
                    return Box::pin(futures::stream::once(async move {
                        Err(Error::StorageReadError(
                            "read_dir on hash store entry",
                            entry.path(),
                            err,
                        ))
                    }));
                }
            },
            Ok(subdir) => subdir,
        };

        Box::pin(try_stream! {
            while let Some(name) = subdir.next_entry().await.map_err(|err| Error::StorageReadError("next_entry on hash store directory", entry.path(), err))? {
                let digest_str = format!("{entry_filename}{}", name.file_name().to_string_lossy());
                if digest_str.ends_with(".completed") {
                    // We're operating on a renders store. These files used to be created
                    // to mark the render as completed before we used atomic renames.
                    continue;
                }

                match encoding::parse_digest(&digest_str) {
                    Ok(digest) => match &search_criteria {
                        crate::graph::DigestSearchCriteria::StartsWith(bytes)
                            if digest.starts_with(bytes.as_slice()) =>
                        {
                            yield digest
                        }
                        crate::graph::DigestSearchCriteria::StartsWith(_) => continue,
                        crate::graph::DigestSearchCriteria::All => yield digest,
                    },
                    Err(err) => {
                        tracing::debug!(
                            ?err, name = ?digest_str,
                            "invalid digest in file storage",
                        );
                    }
                }
            }
        })
    }

    pub fn find(
        &self,
        search_criteria: crate::graph::DigestSearchCriteria,
    ) -> impl Stream<Item = Result<encoding::Digest>> + use<> {
        // Don't capture self inside try_stream.
        let root = self.root.clone();

        try_stream! {
            let mut root_entries = tokio::fs::read_dir(&root).await.map_err(|err| Error::StorageReadError("read_dir on hash store root", root.clone(), err))?;
            while let Some(entry) = root_entries.next_entry().await.map_err(|err| Error::StorageReadError("next_entry on hash store root entry", root.clone(), err))? {
                let entry_filename = entry.file_name();
                let entry_filename = entry_filename.to_string_lossy();

                let mut entry_stream = Self::find_in_entry(search_criteria.clone(), entry).await;
                while let Some(digest) = entry_stream.try_next().await? {
                    yield digest
                }
                drop(entry_stream);

                if let crate::graph::DigestSearchCriteria::StartsWith(partial) = &search_criteria {
                    let encoded = partial.to_string();
                    // we can't trust that the encoded partial digest
                    // references a single subdirectory unless it encodes
                    // to more characters than the filename, because base 32
                    // may encode partial data to the final character
                    let must_be_in_this_folder = encoded.len() > entry_filename.len();
                    if must_be_in_this_folder && encoded.starts_with(entry_filename.as_ref()) {
                        break;
                    }
                }
            }
        }
    }

    pub fn iter(&self) -> impl Stream<Item = Result<encoding::Digest>> + use<> {
        self.find(crate::graph::DigestSearchCriteria::All)
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
        mut reader: Pin<Box<dyn BlobRead>>,
    ) -> Result<(encoding::Digest, u64)> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_file = self.workdir().join(uuid);

        // Enforce that payload files are always written with all read bits
        // enabled so if multiple users are sharing the same repo they don't
        // run into permissions errors reading payloads written by other
        // users.
        let object_permissions = reader.permissions().map(|mode| mode | 0o444);

        self.ensure_base_dir(&working_file)?;
        let mut writer = tokio::io::BufWriter::new(
            tokio::fs::OpenOptions::new()
                .create_new(true)
                .read(true)
                .write(true)
                .open(&working_file)
                .await
                .map_err(|err| {
                    Error::StorageWriteError(
                        "open on hash store object for write",
                        working_file.clone(),
                        err,
                    )
                })?,
        );
        let mut hasher = encoding::Hasher::with_target(&mut writer);
        let copied = match tokio::io::copy(&mut reader, &mut hasher).await {
            Err(err) => {
                let _ = tokio::fs::remove_file(&working_file).await;
                return Err(Error::StorageWriteError(
                    "copy on hash store object file",
                    working_file,
                    err,
                ));
            }
            Ok(s) => s,
        };

        if let Err(err) = hasher.flush().await {
            return Err(Error::StorageWriteError(
                "flush on hash store object file",
                working_file,
                err,
            ));
        }
        let digest = hasher.digest();
        if let Err(err) = writer.into_inner().into_std().await.close() {
            return Err(Error::StorageWriteError(
                "close on hash store object file",
                working_file,
                err,
            ));
        }

        self.persist_object_with_digest(
            PersistableObject::WorkingFile {
                working_file,
                copied,
                object_permissions,
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

        let (copied, created_new_file, object_permissions) = match persistable_object {
            #[cfg(test)]
            PersistableObject::EmptyFile => {
                tokio::fs::OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)
                    .await
                    .map_err(|err| {
                        Error::StorageWriteError(
                            "open on hash store test empty file for write",
                            path.clone(),
                            err,
                        )
                    })?;
                (0, true, None)
            }
            PersistableObject::WorkingFile {
                working_file,
                copied,
                object_permissions,
            } => {
                tracing::trace!(
                    %digest,
                    ?working_file,
                    ?copied,
                    ?object_permissions,
                    "writing object to hash store"
                );
                if let Err(err) = tokio::fs::rename(&working_file, &path).await {
                    let _ = tokio::fs::remove_file(&working_file).await;
                    return Err(Error::StorageWriteError(
                        "rename on hash store object",
                        path,
                        err,
                    ));
                } else {
                    (copied, true, object_permissions)
                }
            }
        };

        // Only set the permissions on a newly created file (by us), and not
        // an existing file. Once written, this file may get hard links of
        // it and those hard links assume that the permissions on the file
        // won't change. For example, writing a payload and then hard linking
        // to that payload in a render that expects the file to have a certain
        // permissions.
        #[cfg(unix)]
        if created_new_file {
            if let Err(err) = tokio::fs::set_permissions(
                &path,
                std::fs::Permissions::from_mode(
                    object_permissions.unwrap_or(self.file_permissions),
                ),
            )
            .await
            {
                if object_permissions.is_some() {
                    // If the caller wanted specific permissions set, then
                    // make it a hard error if set_permissions failed.
                    // XXX: At this time, it doesn't lead to misbehavior if
                    // the permissions aren't changed, but it could cause
                    // extra disk consumption unnecessarily.
                    return Err(Error::StorageWriteError(
                        "set_permissions on object file",
                        path,
                        err,
                    ));
                }

                // not a good enough reason to fail entirely
                #[cfg(feature = "sentry")]
                sentry::capture_event(sentry::protocol::Event {
                    message: Some(format!("{:?}", err)),
                    level: sentry::protocol::Level::Warning,
                    ..Default::default()
                });
            }
        }

        #[cfg(windows)]
        if created_new_file || object_permissions.is_some() {
            // avoid unused variable warning
        }

        Ok((digest, copied))
    }

    pub fn build_digest_path(&self, digest: &encoding::Digest) -> PathBuf {
        let digest_str = digest.to_string();
        self.root.join(&digest_str[..2]).join(&digest_str[2..])
    }

    pub fn ensure_base_dir<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        match path.as_ref().parent() {
            Some(parent) => makedirs_with_perms(parent, self.directory_permissions)
                .map_err(|err| Error::StorageWriteError("ensure_base_dir", parent.to_owned(), err)),
            _ => Ok(()),
        }
    }

    /// Given a valid storage path, get the object digest.
    ///
    /// This method does not validate the path and will provide
    /// invalid references if given an invalid path.
    pub async fn get_digest_from_path<P>(&self, path: P) -> Result<encoding::Digest>
    where
        P: AsRef<Path> + Send + Sync + 'static,
    {
        use std::path::Component::Normal;
        let path = tokio::task::spawn_blocking(move || {
            dunce::canonicalize(&path)
                .map_err(|err| Error::InvalidPath(path.as_ref().to_owned(), err))
        })
        .await
        .expect("task should not panic")?;
        let mut parts: Vec<_> = path.components().collect();
        let last = parts.pop();
        let second_last = parts.pop();
        match (second_last, last) {
            (Some(Normal(a)), Some(Normal(b))) => Ok(encoding::parse_digest(
                a.to_string_lossy() + b.to_string_lossy(),
            )?),
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
                    _ => Err(Error::StorageReadError(
                        "read_dir on full digest path",
                        dirpath.clone(),
                        err,
                    )),
                };
            }
            Ok(mut read_dir) => {
                let mut mapped = Vec::new();
                while let Some(next) = read_dir.next_entry().await.map_err(|err| {
                    Error::StorageReadError("next_entry on full digest path", dirpath.clone(), err)
                })? {
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
            1 => Ok(dirpath.join(options.first().unwrap())),
            _ => Err(Error::AmbiguousReference(short_digest)),
        }
    }

    /// Return the shortened version of the given digest.
    ///
    /// This implementation improves greatly on the base one by limiting
    /// the possible conflicts to a subdirectory (and subset of all digests)
    pub async fn get_shortened_digest(&self, digest: &encoding::Digest) -> Result<String> {
        let filepath = self.build_digest_path(digest);
        let filepath_parent = filepath.parent().ok_or_else(|| {
            Error::String(format!("No parent directory of {}", filepath.display()))
        })?;
        let entries: Vec<_> = match tokio::fs::read_dir(filepath_parent).await {
            Err(err) => {
                return match err.kind() {
                    ErrorKind::NotFound => Err(Error::UnknownObject(*digest)),
                    _ => Err(Error::StorageReadError(
                        "read_dir on shortened digest",
                        filepath_parent.to_owned(),
                        err,
                    )),
                };
            }
            Ok(mut read_dir) => {
                let mut mapped = Vec::new();
                while let Some(next) = read_dir.next_entry().await.map_err(|err| {
                    Error::StorageReadError(
                        "next_entry on shortened digest path",
                        filepath_parent.to_owned(),
                        err,
                    )
                })? {
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
