// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::Stream;
use nix::fcntl::OFlag;
use nix::sys::stat::Mode;
use tokio::io::AsyncReadExt;

use super::FSRepository;
use crate::encoding::{self, Encodable};
use crate::runtime::makedirs_with_perms;
use crate::storage::{ManifestViewer, PayloadStorage, Repository};
use crate::{graph, tracking, Error, Result};

#[cfg(test)]
#[path = "./renderer_test.rs"]
mod renderer_test;

#[derive(Debug, Copy, Clone)]
pub enum RenderType {
    HardLink,
    Copy,
}

#[async_trait::async_trait]
impl ManifestViewer for FSRepository {
    async fn has_rendered_manifest(&self, digest: encoding::Digest) -> bool {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return false,
        };
        let rendered_dir = renders.build_digest_path(&digest);
        was_render_completed(rendered_dir)
    }

    fn iter_rendered_manifests<'db>(
        &'db self,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + 'db>> {
        Box::pin(try_stream! {
            let renders = self.get_render_storage()?;
            for await digest in renders.iter() {
                yield digest?;
            }
        })
    }

    /// Return the path that the manifest would be rendered to.
    fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<PathBuf> {
        Ok(self
            .get_render_storage()?
            .build_digest_path(&manifest.digest()?))
    }

    fn proxy_path(&self) -> Option<&std::path::Path> {
        self.renders
            .as_ref()
            .map(|render_store| render_store.proxy.root())
    }

    /// Create a hard-linked rendering of the given file manifest.
    ///
    /// # Errors:
    /// - if any of the blobs in the manifest are not available in this repo.
    async fn render_manifest(&self, manifest: &graph::Manifest) -> Result<PathBuf> {
        let renders = self.get_render_storage()?;
        let rendered_dirpath = renders.build_digest_path(&manifest.digest()?);
        if was_render_completed(&rendered_dirpath) {
            tracing::trace!(path = ?rendered_dirpath, "render already completed");
            return Ok(rendered_dirpath);
        }
        tracing::trace!(path = ?rendered_dirpath, "rendering manifest...");

        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dir = renders.workdir().join(uuid);
        makedirs_with_perms(&working_dir, 0o777)?;

        self.render_manifest_into_dir(manifest, &working_dir, RenderType::HardLink)
            .await?;

        renders.ensure_base_dir(&rendered_dirpath)?;
        match tokio::fs::rename(&working_dir, &rendered_dirpath).await {
            Ok(_) => (),
            Err(err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    if let Err(err) = open_perms_and_remove_all(&working_dir).await {
                        tracing::warn!(path=?working_dir, "failed to clean up working directory: {:?}", err);
                    }
                }
                _ => {
                    return Err(Error::StorageWriteError(
                        "rename on render",
                        rendered_dirpath,
                        err,
                    ))
                }
            },
        }

        mark_render_completed(&rendered_dirpath).await?;
        Ok(rendered_dirpath)
    }

    /// Remove the identified render from this storage.
    async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()> {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return Ok(()),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dirpath = renders.workdir().join(uuid);
        renders.ensure_base_dir(&working_dirpath)?;
        if let Err(err) = tokio::fs::rename(&rendered_dirpath, &working_dirpath).await {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(crate::Error::StorageWriteError(
                    "rename on render before removal",
                    working_dirpath,
                    err,
                )),
            };
        }

        unmark_render_completed(&rendered_dirpath).await?;
        open_perms_and_remove_all(&working_dirpath).await
    }

    async fn remove_rendered_manifest_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<()> {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return Ok(()),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);

        let metadata = match tokio::fs::symlink_metadata(&rendered_dirpath).await {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => {
                return Err(Error::StorageReadError(
                    "symlink_metadata on rendered dir path",
                    rendered_dirpath.clone(),
                    err,
                ))
            }
            Ok(metadata) => metadata,
        };

        let mtime = metadata.modified().map_err(|err| {
            Error::StorageReadError(
                "modified on symlink metadata of rendered dir path",
                rendered_dirpath.clone(),
                err,
            )
        })?;

        if DateTime::<Utc>::from(mtime) >= older_than {
            return Ok(());
        }

        self.remove_rendered_manifest(digest).await
    }
}

impl FSRepository {
    fn get_render_storage(&self) -> Result<&super::FSHashStore> {
        match &self.renders {
            Some(render_store) => Ok(&render_store.renders),
            None => Err(Error::NoRenderStorage(self.address())),
        }
    }

    pub async fn render_manifest_into_dir<P: AsRef<Path>>(
        &self,
        manifest: &graph::Manifest,
        target_dir: P,
        render_type: RenderType,
    ) -> Result<()> {
        let root_node = manifest.root();
        let target_dir = target_dir.as_ref();
        tokio::fs::create_dir_all(target_dir).await.map_err(|err| {
            Error::StorageWriteError(
                "creating the root render directory",
                target_dir.to_owned(),
                err,
            )
        })?;
        // because we open with O_DIRECTORY and O_PATH, the mode and other
        // access flags are ignored
        let root_dir_fd = nix::fcntl::open(
            target_dir,
            OFlag::O_DIRECTORY | OFlag::O_PATH,
            Mode::empty(),
        )?;

        let mut res = self
            .render_into_dir_fd(root_dir_fd, root_node, manifest, render_type)
            .await;
        if let Err(Error::StorageWriteError(_, p, _)) = &mut res {
            *p = target_dir.join(p.as_path());
        }
        res
    }

    #[async_recursion::async_recursion]
    pub async fn render_into_dir_fd<Fd>(
        &self,
        root_dir_fd: Fd,
        tree: &graph::Tree,
        manifest: &graph::Manifest,
        render_type: RenderType,
    ) -> Result<()>
    where
        Fd: AsRawFd + Send,
    {
        let root_dir_fd = root_dir_fd.as_raw_fd();

        // we used to get CAP_FOWNER here, but with async
        // it can no longer guarantee anything useful
        // (the process can happen in other threads, and
        // other code can run in the current thread).
        // Instead, we try to rely on generating the structure
        // first with open permissions and then locking it down

        for entry in tree.entries.iter() {
            let res = match entry.kind {
                tracking::EntryKind::Tree => {
                    let tree = manifest.get_tree(&entry.object).ok_or_else(|| {
                        Error::String(format!("Failed to render: manifest is internally inconsistent (missing child tree {})", entry.object))
                    })?;

                    let child_dir_fd = create_and_open_dir_at(root_dir_fd, entry.name.clone())
                        .await
                        .map_err(|err| {
                            Error::StorageWriteError(
                                "create dir during render",
                                PathBuf::from(&entry.name),
                                err,
                            )
                        })?;
                    let mut res = self
                        .render_into_dir_fd(child_dir_fd, tree, manifest, render_type)
                        .await;
                    if res.is_ok() {
                        res = nix::sys::stat::fchmod(
                            child_dir_fd,
                            Mode::from_bits_truncate(entry.mode),
                        )
                        .map_err(|err| {
                            Error::StorageWriteError(
                                "set_permissions on rendered dir",
                                PathBuf::new(),
                                err.into(),
                            )
                        });
                    }
                    res
                }
                tracking::EntryKind::Mask => continue,
                tracking::EntryKind::Blob => {
                    self.render_blob(root_dir_fd, entry, render_type).await
                }
            };
            if let Err(mut err) = res {
                if let Error::StorageWriteError(_, p, _) = &mut err {
                    *p = Path::new(&entry.name).join(p.as_path());
                }
                return Err(err);
            }
        }

        Ok(())
    }

    /// Renders the file into a path on disk, changing its permissions
    /// as necessary / appropriate
    async fn render_blob<'a, Fd: std::os::fd::AsRawFd>(
        &self,
        dir_fd: Fd,
        entry: &graph::Entry,
        render_type: RenderType,
    ) -> Result<()> {
        let target_dir_fd = dir_fd.as_raw_fd();
        if entry.is_symlink() {
            let (mut reader, filename) = self.open_payload(entry.object).await?;
            let mut target = String::new();
            reader.read_to_string(&mut target).await.map_err(|err| {
                Error::StorageReadError("read_to_string on render blob", filename, err)
            })?;
            return if let Err(err) =
                nix::unistd::symlinkat(target.as_str(), Some(target_dir_fd), entry.name.as_str())
            {
                match err {
                    nix::errno::Errno::EEXIST => Ok(()),
                    _ => Err(Error::StorageWriteError(
                        "symlink on rendered blob",
                        PathBuf::from(&entry.name),
                        err.into(),
                    )),
                }
            } else {
                Ok(())
            };
        }

        let mut committed_path = self.payloads.build_digest_path(&entry.object);
        match render_type {
            RenderType::HardLink => {
                let payload_path = committed_path;
                let mut retry_count = 0;
                loop {
                    // All hard links to a file have shared metadata (owner, perms).
                    // Whereas the same blob may be rendered into multiple files
                    // across different users and/or will different expected perms.
                    // Therefore, a copy of the blob is needed for every unique
                    // combination of user and perms. Since each user has their own
                    // "proxy" directory, there needs only be a unique copy per
                    // perms.
                    if let Some(render_store) = &self.renders {
                        let proxy_path = render_store
                            .proxy
                            .build_digest_path(&entry.object)
                            .join(entry.mode.to_string());
                        tracing::trace!(?proxy_path, "proxy");
                        if !proxy_path.exists() {
                            let path_to_create = proxy_path.parent().unwrap();
                            tokio::fs::create_dir_all(&path_to_create)
                                .await
                                .map_err(|err| {
                                    Error::StorageWriteError(
                                        "create_dir_all on blob proxy base",
                                        path_to_create.to_owned(),
                                        err,
                                    )
                                })?;
                            // Write to a temporary file so that some other render
                            // process doesn't think a partially-written file is
                            // good.
                            let temp_proxy_file = tempfile::NamedTempFile::new_in(path_to_create)
                                .map_err(|err| {
                                Error::StorageWriteError(
                                    "create proxy temp file",
                                    path_to_create.to_owned(),
                                    err,
                                )
                            })?;
                            let payload_fd =
                                nix::fcntl::open(&payload_path, OFlag::O_RDONLY, Mode::empty())?;
                            let proxy_file_fd = temp_proxy_file.as_file().as_raw_fd();
                            copy_fd(payload_fd, proxy_file_fd).await.map_err(|err| {
                                Error::StorageWriteError(
                                    "copy of blob to proxy file",
                                    temp_proxy_file.path().to_owned(),
                                    err,
                                )
                            })?;
                            nix::sys::stat::fchmod(
                                proxy_file_fd,
                                Mode::from_bits_truncate(entry.mode),
                            )
                            .map_err(|err| {
                                Error::StorageWriteError(
                                    "set permissions on proxy payload",
                                    PathBuf::from(&entry.name),
                                    err.into(),
                                )
                            })?;
                            // Move temporary file into place.
                            if let Err(err) = temp_proxy_file.persist_noclobber(&proxy_path) {
                                match err.error.kind() {
                                    std::io::ErrorKind::AlreadyExists => (),
                                    _ => {
                                        return Err(Error::StorageWriteError(
                                            "persist of blob proxy file",
                                            proxy_path.to_owned(),
                                            err.error,
                                        ))
                                    }
                                }
                            }
                        }
                        // Renders should hard link to this proxy file; it will
                        // be owned by the current user and (eventually) have the
                        // expected mode.
                        committed_path = proxy_path;
                    } else {
                        return Err(
                            "Cannot render blob as hard link to repository with no render store"
                                .into(),
                        );
                    }

                    if let Err(err) = nix::unistd::linkat(
                        None,
                        committed_path.as_path(),
                        Some(target_dir_fd),
                        std::path::Path::new(&entry.name),
                        nix::unistd::LinkatFlags::NoSymlinkFollow,
                    ) {
                        match err {
                            nix::errno::Errno::ENOENT if retry_count < 3 => {
                                // There is a chance to lose a race with
                                // `spfs clean` if it sees `committed_path` as
                                // only having one link. If we get a `NotFound`
                                // error, assume our newly copied file has
                                // been deleted and try again.
                                //
                                // It's _very_ unlikely we'd lose this race
                                // multiple times. Don't loop forever.
                                retry_count += 1;
                                continue;
                            }
                            nix::errno::Errno::EEXIST => (),
                            _ => {
                                return Err(Error::StorageWriteError(
                                    "hard_link of blob proxy to rendered path",
                                    PathBuf::from(&entry.name),
                                    err.into(),
                                ))
                            }
                        }
                    }

                    break;
                }
            }
            RenderType::Copy => {
                let payload_fd = nix::fcntl::open(&committed_path, OFlag::O_RDONLY, Mode::empty())?;
                // create with open permissions, as they will be set to the proper mode in the future
                let rendered_fd = nix::fcntl::openat(
                    target_dir_fd,
                    entry.name.as_str(),
                    OFlag::O_RDWR | OFlag::O_CREAT,
                    Mode::all(),
                )
                .map_err(|err| {
                    Error::StorageWriteError(
                        "creation of rendered blob file",
                        PathBuf::from(&entry.name),
                        err.into(),
                    )
                })?;
                copy_fd(payload_fd, rendered_fd).await.map_err(|err| {
                    Error::StorageWriteError(
                        "copy of blob to rendered file",
                        PathBuf::from(&entry.name),
                        err,
                    )
                })?;
                return nix::sys::stat::fchmod(rendered_fd, Mode::from_bits_truncate(entry.mode))
                    .map_err(|err| {
                        Error::StorageWriteError(
                            "set permissions on copied payload",
                            PathBuf::from(&entry.name),
                            err.into(),
                        )
                    });
            }
        }
        Ok(())
    }
}

async fn create_and_open_dir_at<A>(dir_fd: A, name: String) -> std::io::Result<i32>
where
    A: AsRawFd + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        // leave the permissions open for now, so that
        // the structure inside can be generated without
        // privileged access
        match nix::sys::stat::mkdirat(dir_fd.as_raw_fd(), name.as_str(), Mode::all()) {
            Ok(_) | Err(nix::errno::Errno::EEXIST) => {}
            Err(err) => return Err(err.into()),
        }
        nix::fcntl::openat(
            dir_fd.as_raw_fd(),
            name.as_str(),
            OFlag::O_DIRECTORY | OFlag::O_RDONLY,
            Mode::all(),
        )
        .map_err(std::io::Error::from)
    })
    .await
    .map_err(|_join_err| std::io::Error::new(std::io::ErrorKind::Other, "mkdir task panic'd"))?
}

async fn copy_fd<Fd1, Fd2>(from_fd: Fd1, to_fd: Fd2) -> std::io::Result<u64>
where
    Fd1: AsRawFd + Send,
    Fd2: AsRawFd + Send,
{
    let from_fd = nix::unistd::dup(from_fd.as_raw_fd())?;
    let to_fd = nix::unistd::dup(to_fd.as_raw_fd())?;
    // Safety: from_raw_fd takes ownership of the fd, but we
    // are duplicating to ensure that this is guaranteed
    let mut from = unsafe { tokio::fs::File::from_raw_fd(from_fd) };
    let mut to = unsafe { tokio::fs::File::from_raw_fd(to_fd) };
    // std::io::copy will try to use more efficient kernel functions if possible
    let copied = tokio::io::copy(&mut from, &mut to).await?;
    tokio::try_join!(from.sync_data(), to.sync_data())?;
    Ok(copied)
}

/// Walks down a filesystem tree, opening permissions on each file before removing
/// the entire tree.
///
/// This process handles the case when a folder may include files
/// that need to be removed but on which the user doesn't have enough permissions.
/// It does assume that the current user owns the file, as it may not be possible to
/// change permissions before removal otherwise.
#[async_recursion::async_recursion]
async fn open_perms_and_remove_all(root: &Path) -> Result<()> {
    let mut read_dir = tokio::fs::read_dir(&root)
        .await
        .map_err(|err| Error::StorageReadError("read_dir on root", root.to_owned(), err))?;
    // TODO: parallelize this with async
    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|err| Error::StorageReadError("next_entry on root dir", root.to_owned(), err))?
    {
        let entry_path = root.join(entry.file_name());
        let file_type = entry.file_type().await.map_err(|err| {
            Error::StorageReadError("file_type on root entry", root.to_owned(), err)
        })?;
        let _ =
            tokio::fs::set_permissions(&entry_path, std::fs::Permissions::from_mode(0o777)).await;
        if file_type.is_symlink() || file_type.is_file() {
            tokio::fs::remove_file(&entry_path).await.map_err(|err| {
                Error::StorageWriteError("remove_file on render entry", entry_path.clone(), err)
            })?;
        }
        if file_type.is_dir() {
            open_perms_and_remove_all(&entry_path).await?;
        }
    }
    tokio::fs::remove_dir(&root).await.map_err(|err| {
        Error::StorageWriteError("remove_dir on render root", root.to_owned(), err)
    })?;
    Ok(())
}

fn was_render_completed<P: AsRef<Path>>(render_path: P) -> bool {
    let mut name = render_path
        .as_ref()
        .file_name()
        .expect("must have a file name")
        .to_os_string();
    name.push(".completed");
    let marker_path = render_path.as_ref().with_file_name(name);
    marker_path.exists()
}

/// panics if the given path does not have a directory name
async fn mark_render_completed<P: AsRef<Path>>(render_path: P) -> Result<()> {
    let mut name = render_path
        .as_ref()
        .file_name()
        .expect("must have a file name")
        .to_os_string();
    name.push(".completed");
    let marker_path = render_path.as_ref().with_file_name(name);
    // create if it doesn't exist but don't fail if it already exists (no exclusive open)
    tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&marker_path)
        .await
        .map_err(|err| {
            Error::StorageWriteError(
                "open render completed marker path for write",
                marker_path,
                err,
            )
        })?;
    Ok(())
}

async fn unmark_render_completed<P: AsRef<Path>>(render_path: P) -> Result<()> {
    let mut name = render_path
        .as_ref()
        .file_name()
        .expect("must have a file name")
        .to_os_string();
    name.push(".completed");
    let marker_path = render_path.as_ref().with_file_name(name);
    if let Err(err) = tokio::fs::remove_file(&marker_path).await {
        match err.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(Error::StorageWriteError(
                "remove file on render completed marker",
                marker_path,
                err,
            )),
        }
    } else {
        Ok(())
    }
}
