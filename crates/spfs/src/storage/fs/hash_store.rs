// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ffi::OsStr;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::runtime::makedirs_with_perms;
use crate::{encoding, graph, Error, Result};

static WORK_DIRNAME: &str = "work";

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

    pub fn iter<'a>(&'a self) -> Result<FSHashStoreIter> {
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
    pub fn write_data(
        &mut self,
        mut reader: Box<&mut dyn std::io::Read>,
    ) -> Result<(encoding::Digest, u64)> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_file = self.workdir().join(uuid);

        self.ensure_base_dir(&working_file)?;
        let mut writer = std::fs::OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&working_file)?;
        let mut hasher = encoding::Hasher::new().with_target(&mut writer);
        let copied = match std::io::copy(&mut reader, &mut hasher) {
            Err(err) => {
                let _ = std::fs::remove_file(working_file);
                return Err(Error::wrap_io(err, "Failed to write object data"));
            }
            Ok(s) => s,
        };

        let digest = hasher.digest();
        let path = self.build_digest_path(&digest);
        self.ensure_base_dir(&path)?;
        match std::fs::rename(&working_file, &path) {
            Err(err) => {
                let _ = std::fs::remove_file(working_file);
                match err.kind() {
                    ErrorKind::AlreadyExists => (),
                    _ => return Err(Error::wrap_io(err, "Failed to store object")),
                }
            }
            Ok(_) => (),
        }
        if let Err(err) = std::fs::set_permissions(
            &path,
            std::fs::Permissions::from_mode(self.file_permissions),
        ) {
            // not a good enough reason to fail entirely
            sentry::capture_event(sentry::protocol::Event {
                message: Some(format!("{:?}", err)),
                level: sentry::protocol::Level::Warning,
                ..Default::default()
            });
            tracing::warn!("Failed to set object permissions: {:?}", err);
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
    pub fn get_digest_from_path<P: AsRef<Path>>(&self, path: P) -> Result<encoding::Digest> {
        use std::path::Component::Normal;
        let path = std::fs::canonicalize(path)?;
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
    /// - graph::UnknownObjectError: if the digest cannot be resolved
    /// - graph::AmbiguousReferenceError: if the digest resolves to more than one path
    pub fn resolve_full_digest_path(&self, partial: &encoding::PartialDigest) -> Result<PathBuf> {
        let short_digest = partial.to_string();
        let (dirname, file_prefix) = (&short_digest[..2], &short_digest[2..]);
        let dirpath = self.root.join(dirname);
        if short_digest.len() == encoding::DIGEST_SIZE {
            return Ok(dirpath.join(file_prefix));
        }

        let entries: Vec<std::ffi::OsString> = match std::fs::read_dir(&dirpath) {
            Err(err) => {
                return match err.kind() {
                    ErrorKind::NotFound => Err(graph::UnknownReferenceError::new(short_digest)),
                    _ => Err(err.into()),
                }
            }
            Ok(read_dir) => {
                let mapped: Result<Vec<_>> = read_dir
                    .map(|d| match d {
                        Ok(e) => Ok(e.file_name()),
                        Err(e) => Err(Error::from(e)),
                    })
                    .collect();
                mapped?
            }
        };

        let options: Vec<std::ffi::OsString> = entries
            .into_iter()
            .filter(|x| x.to_string_lossy().starts_with(file_prefix))
            .collect();
        match options.len() {
            0 => Err(graph::UnknownReferenceError::new(short_digest)),
            1 => Ok(dirpath.join(options.get(0).unwrap())),
            _ => Err(graph::AmbiguousReferenceError::new(short_digest)),
        }
    }

    /// Return the shortened version of the given digest.
    ///
    /// This implementation improves greatly on the base one by limiting
    /// the possible conflicts to a subdirectory (and subset of all digests)
    pub fn get_shortened_digest(&self, digest: &encoding::Digest) -> Result<String> {
        let filepath = self.build_digest_path(digest);
        let entries: Vec<_> = match std::fs::read_dir(filepath.parent().unwrap()) {
            Err(err) => {
                return match err.kind() {
                    ErrorKind::NotFound => Err(graph::UnknownObjectError::new(digest)),
                    _ => Err(err.into()),
                };
            }
            Ok(read_dir) => {
                let mapped: Result<Vec<_>> = read_dir
                    .map(|d| match d {
                        Ok(e) => Ok(e.file_name().to_string_lossy().to_string()),
                        Err(e) => Err(Error::from(e)),
                    })
                    .collect();
                mapped?
            }
        };

        let digest_str = digest.to_string();
        let mut shortest_size = 8;
        let mut shortest = &digest_str[2..shortest_size];
        for other in entries {
            if &other[0..shortest_size] != shortest {
                continue;
            } else if other == digest_str[2..] {
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
    /// - graph::UnknownObjectError: if the digest cannot be resolved
    /// - graph::AmbiguousReferenceError: if the digest resolves to more than one path
    pub fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
    ) -> Result<encoding::Digest> {
        if let Some(complete) = partial.to_digest() {
            return Ok(complete);
        }
        let path = self.resolve_full_digest_path(partial)?;
        self.get_digest_from_path(path)
    }
}

pub struct FSHashStoreIter {
    root: PathBuf,
    root_readdir: std::fs::ReadDir,
    active_readdir: Option<(std::ffi::OsString, std::fs::ReadDir)>,
}

impl FSHashStoreIter {
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let path = std::fs::canonicalize(root)?;
        Ok(Self {
            root: path.clone(),
            root_readdir: std::fs::read_dir(&path)?,
            active_readdir: None,
        })
    }
}

impl Iterator for FSHashStoreIter {
    type Item = Result<encoding::Digest>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.active_readdir.as_mut() {
                None => {
                    let next_dir = match self.root_readdir.next() {
                        Some(Ok(res)) => res,
                        Some(Err(err)) => return Some(Err(err.into())),
                        None => return None,
                    };
                    let prefix = next_dir.file_name();
                    if prefix.as_os_str() == OsStr::new(WORK_DIRNAME) {
                        continue;
                    }
                    let path = self.root.join(&prefix);
                    match std::fs::read_dir(&path) {
                        Ok(read_dir) => {
                            self.active_readdir.replace((prefix, read_dir));
                        }
                        Err(err) => match err.raw_os_error() {
                            Some(libc::ENOTDIR) => {
                                tracing::debug!(path = ?path, "found non-directory in hash storage");
                                continue;
                            }
                            _ => break Some(Err(err.into())),
                        },
                    }
                    continue;
                }
                Some((prefix, read_dir)) => match read_dir.next() {
                    None => {
                        self.active_readdir.take();
                        continue;
                    }
                    Some(Err(err)) => break Some(Err(err.into())),
                    Some(Ok(entry)) => {
                        let mut digest_str = prefix.to_string_lossy().to_string();
                        digest_str.push_str(entry.file_name().to_string_lossy().as_ref());
                        break match encoding::parse_digest(&digest_str) {
                            Ok(digest) => Some(Ok(digest)),
                            Err(err) => Some(Err(format!(
                                "invalid digest in file storage: {:?} [{:?}]",
                                err, digest_str
                            )
                            .into())),
                        };
                    }
                },
            }
        }
    }
}
