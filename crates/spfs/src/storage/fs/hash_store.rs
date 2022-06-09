// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_stream::try_stream;
use close_err::Closable;
use futures::Stream;
use tokio::io::AsyncWriteExt;

use crate::runtime::makedirs_with_perms;
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./hash_store_test.rs"]
mod hash_store_test;

const WORK_DIRNAME: &str = "work";

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

    pub fn find(
        &self,
        search_criteria: crate::graph::DigestSearchCriteria,
    ) -> impl Stream<Item = Result<encoding::Digest>> {
        // Don't capture self inside try_stream.
        let root = self.root.clone();

        try_stream! {
            let mut root = tokio::fs::read_dir(&root).await?;
            while let Some(entry) = root.next_entry().await? {
                let entry_filename = entry.file_name();
                let entry_filename = entry_filename.to_string_lossy();
                if entry_filename == WORK_DIRNAME {
                    continue;
                }

                let entry_partial = match encoding::PartialDigest::parse(&entry_filename) {
                    Err(err) => {
                        tracing::debug!(
                            ?err, "invalid digest directory in file storage",
                        );
                        continue;
                    },
                    Ok(partial) => partial,
                };
                let entry_bytes = entry_partial.as_slice();

                match &search_criteria {
                    crate::graph::DigestSearchCriteria::StartsWith(bytes)
                        // If the directory name is shorter than the prefix, check that
                        // the prefix starts with the directory name.
                        if entry_bytes.len() < bytes.len() && !bytes.starts_with(entry_bytes) => {
                            continue;
                        }
                    crate::graph::DigestSearchCriteria::StartsWith(bytes)
                        // If the directory name is longer than the prefix, check that
                        // the directory name starts with the prefix.
                        if entry_bytes.len() >= bytes.len() && !entry_bytes.starts_with(bytes) => {
                            continue;
                        }
                    _ => {
                    }
                };

                let mut subdir = match tokio::fs::read_dir(entry.path()).await {
                    Err(err) => match err.raw_os_error() {
                        Some(libc::ENOTDIR) => {
                            tracing::debug!(?entry_filename, "found non-directory in hash storage");
                            continue;
                        }
                        _ => Err(err)?,
                    }
                    Ok(subdir) => subdir,
                };
                while let Some(name) = subdir.next_entry().await? {
                    let digest_str = format!("{entry_filename}{}", name.file_name().to_string_lossy());

                    match encoding::parse_digest(&digest_str) {
                        Ok(digest) => match &search_criteria {
                            crate::graph::DigestSearchCriteria::StartsWith(bytes) if digest.starts_with(bytes.as_slice()) => yield digest,
                            crate::graph::DigestSearchCriteria::StartsWith(_) => continue,
                            crate::graph::DigestSearchCriteria::All => yield digest,
                        }
                        Err(err) => {
                            tracing::debug!(
                                ?err, name = ?digest_str,
                                "invalid digest in file storage",
                            );
                        }
                    }
                }

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

    pub fn iter(&self) -> impl Stream<Item = Result<encoding::Digest>> {
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
            return Err(Error::wrap_io(err, "Failed to flush object write"));
        }
        if let Err(err) = writer.into_inner().into_std().await.close() {
            return Err(Error::wrap_io(err, "Failed to close object file"));
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
