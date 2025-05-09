// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::prelude::MetadataExt;
use std::path::{Path, PathBuf};

use futures::future::ready;
use futures::{FutureExt, StreamExt};
use nix::fcntl::OFlag;
use nix::sys::stat::Mode;
use nix::unistd::geteuid;
use rand::prelude::*;
use tokio::io::AsyncReadExt;

use super::{BlobSemaphorePermit, RenderType, Renderer};
use crate::prelude::*;
use crate::storage::LocalRepository;
use crate::storage::fs::RenderReporter;
use crate::storage::fs::render_reporter::RenderBlobResult;
use crate::{Error, OsError, Result, get_config, graph, tracking};

impl<Repo, Reporter> Renderer<'_, Repo, Reporter>
where
    Repo: Repository + LocalRepository,
    Reporter: RenderReporter,
{
    /// Recreate the full structure of a stored manifest on disk.
    pub async fn render_manifest_into_dir<P>(
        &self,
        manifest: &graph::Manifest,
        target_dir: P,
        render_type: RenderType,
    ) -> Result<()>
    where
        P: AsRef<Path>,
    {
        self.reporter.visit_layer(manifest);
        let root_node = manifest.root();
        let target_dir = target_dir.as_ref();
        tokio::fs::create_dir_all(target_dir).await.map_err(|err| {
            Error::StorageWriteError(
                "creating the root render directory",
                target_dir.to_owned(),
                err,
            )
        })?;
        let path = target_dir.to_owned();
        let root_dir = tokio::task::spawn_blocking(move || -> Result<tokio::fs::File> {
            let fd = nix::fcntl::open(&path, OFlag::O_DIRECTORY | OFlag::O_PATH, Mode::empty())
                .map_err(|err| {
                    Error::StorageWriteError("open render target dir", path, err.into())
                })?;
            // Safety: from_raw_fd takes ownership of this fd which is what we want
            Ok(unsafe { tokio::fs::File::from_raw_fd(fd) })
        })
        .await
        .expect("syscall should not panic")?;

        for entry in manifest.iter_entries() {
            self.reporter.visit_entry(entry);
        }

        let manifest_tree_cache = manifest.get_tree_cache();
        let mut res = self
            .render_into_dir_fd(root_dir, &root_node, &manifest_tree_cache, render_type)
            .await;
        if let Err(Error::StorageWriteError(_, p, _)) = &mut res {
            *p = target_dir.join(p.as_path());
        }
        res.map_err(|err| err.wrap("render_into_dir <root node>"))?;
        self.reporter.rendered_layer(manifest);
        Ok(())
    }

    #[async_recursion::async_recursion]
    async fn render_into_dir_fd<Fd>(
        &self,
        root_dir_fd: Fd,
        tree: &graph::Tree<'async_recursion>,
        manifest_tree_cache: &graph::ManifestTreeCache,
        render_type: RenderType,
    ) -> Result<()>
    where
        Fd: AsRawFd + Send + Sync,
    {
        // we used to get CAP_FOWNER here, but with async
        // it can no longer guarantee anything useful
        // (the process can happen in other threads, and
        // other code can run in the current thread).
        // Instead, we try to rely on generating the structure
        // first with open permissions and then locking it down

        // randomize the entries so that multiple processes starting
        // at the same time don't have as much contention trying to render
        // the same files as one another at the same time. This can happen,
        // for example, when multiple frames land on the same machine in a
        // render farm and they both start to render the same env at the same time
        let mut entries = tree.entries().collect::<Vec<_>>();
        entries.shuffle(&mut rand::thread_rng());

        let root_dir_fd = root_dir_fd.as_raw_fd();
        let mut stream = futures::stream::iter(entries)
            .then(move |entry| {
                let fut = async move {
                    let mut root_path = PathBuf::from(entry.name());
                    match entry.kind() {
                        tracking::EntryKind::Tree => {
                            let tree = manifest_tree_cache.get(entry.object()).ok_or_else(|| {
                                Error::String(format!("Failed to render: manifest is internally inconsistent (missing child tree {})", *entry.object()))
                            })?;

                            let child_dir = create_and_open_dir_at(root_dir_fd, entry.name().to_owned())
                                .await
                                .map_err(|err| {
                                    Error::StorageWriteError(
                                        "create dir during render",
                                        PathBuf::from(entry.name()),
                                        err,
                                    )
                                })?;
                            let mut res = self
                                .render_into_dir_fd(
                                    child_dir.as_raw_fd(),
                                    tree,
                                    manifest_tree_cache,
                                    render_type,
                                )
                                .await;
                            if res.is_ok() {
                                let mode = Mode::from_bits_truncate(entry.mode());
                                res = tokio::task::spawn_blocking(move || {
                                    nix::sys::stat::fchmod(
                                        child_dir.as_raw_fd(),
                                        mode,
                                    )
                                })
                                .await
                                .expect("syscall should not panic")
                                .map_err(|err| {
                                    Error::StorageWriteError(
                                        "set_permissions on rendered dir",
                                        PathBuf::new(),
                                        err.into(),
                                    )
                                });
                            }
                            if let Err(Error::StorageWriteError(_, p, _)) = &mut res {
                                root_path.push(p.as_path());
                                *p = root_path;
                            }
                            res.map(|_| None).map_err(|err| err.wrap(format!("render_into_dir '{}'", entry.name())))
                        }
                        tracking::EntryKind::Mask => Ok(None),
                        tracking::EntryKind::Blob(_) => {
                            self.render_blob(root_dir_fd, entry, render_type).await.map(Some).map_err(|err| err.wrap(format!("render blob '{}'", entry.name())))
                        }
                    }.map(|render_blob_result_opt| (entry, render_blob_result_opt))
                };
                ready(fut.boxed())
            })
            .buffer_unordered(self.max_concurrent_branches)
            .boxed();

        while let Some(res) = stream.next().await {
            match res {
                Err(error) => return Err(error),
                Ok((entry, Some(render_blob_result))) => {
                    self.reporter.rendered_blob(entry, &render_blob_result);
                    self.reporter.rendered_entry(entry);
                }
                Ok((entry, _)) => self.reporter.rendered_entry(entry),
            }
        }

        Ok(())
    }

    /// Renders the file into a path on disk, changing its permissions
    /// as necessary / appropriate
    async fn render_blob<Fd>(
        &self,
        dir_fd: Fd,
        entry: graph::Entry<'_>,
        render_type: RenderType,
    ) -> Result<RenderBlobResult>
    where
        Fd: std::os::fd::AsRawFd + Send,
    {
        let permit = self.blob_semaphore.acquire().await;
        self.render_blob_with_permit(dir_fd, entry, render_type, permit)
            .await
    }

    /// Render a single blob onto disk
    #[async_recursion::async_recursion]
    #[allow(clippy::only_used_in_recursion)]
    async fn render_blob_with_permit<'a, Fd>(
        &self,
        dir_fd: Fd,
        entry: graph::Entry<'async_recursion>,
        render_type: RenderType,
        permit: BlobSemaphorePermit<'a>,
    ) -> Result<RenderBlobResult>
    where
        'a: 'async_recursion,
        Fd: std::os::fd::AsRawFd + Send,
    {
        // Note that opening the payload, even if the return value is not
        // used, has a possible side effect of repairing a missing payload,
        // depending on repository implementation.
        // When the blob is not a symlink, the code below will try to access
        // the payload file without calling `open_payload`. If `open_payload`
        // is not called here, the non-symlink code may fail due to a missing
        // a payload that could have been repaired.
        let (mut reader, filename) = self
            .repo
            .open_payload(*entry.object())
            .await
            .map_err(|err| err.wrap("open payload"))?;
        let target_dir_fd = dir_fd.as_raw_fd();
        if entry.is_symlink() {
            let mut target = String::new();
            {
                reader.read_to_string(&mut target).await.map_err(|err| {
                    Error::StorageReadError("read_to_string on render blob", filename, err)
                })?;
            }
            return if let Err(err) =
                nix::unistd::symlinkat(target.as_str(), Some(target_dir_fd), entry.name())
            {
                match err {
                    nix::errno::Errno::EEXIST => Ok(RenderBlobResult::SymlinkAlreadyExists),
                    _ => Err(Error::StorageWriteError(
                        "symlink on rendered blob",
                        PathBuf::from(entry.name()),
                        err.into(),
                    )),
                }
            } else {
                Ok(RenderBlobResult::SymlinkWritten)
            };
        }
        // Free up file resources as early as possible.
        drop(reader);

        let mut committed_path = self.repo.payloads().build_digest_path(entry.object());
        Ok(match render_type {
            RenderType::HardLink | RenderType::HardLinkNoProxy => {
                let mut retry_count = 0;
                loop {
                    let payload_path = committed_path.clone();
                    // All hard links to a file have shared metadata (owner, perms).
                    // Whereas the same blob may be rendered into multiple files
                    // across different users and/or will different expected perms.
                    // Therefore, a copy of the blob is needed for every unique
                    // combination of user and perms. Since each user has their own
                    // "proxy" directory, there needs only be a unique copy per
                    // perms.
                    let render_blob_result = if matches!(render_type, RenderType::HardLinkNoProxy) {
                        // explicitly skip proxy generation
                        RenderBlobResult::PayloadCopiedByRequest
                    } else if let Ok(render_store) = self.repo.render_store() {
                        let proxy_path = render_store
                            .proxy
                            .build_digest_path(entry.object())
                            .join(entry.mode().to_string());
                        tracing::trace!(?proxy_path, "proxy");
                        let render_blob_result = if !proxy_path.exists() {
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

                            // To save on disk space, if the payload already
                            // has the expected owner and perms, then the
                            // proxy can be a hard link instead of a copy.
                            // This assumes that the payload's owner and
                            // permissions are never changed.
                            let metadata = match tokio::fs::symlink_metadata(&payload_path).await {
                                Err(err) => {
                                    return Err(Error::StorageReadError(
                                        "symlink_metadata on payload path",
                                        payload_path.clone(),
                                        err,
                                    ));
                                }
                                Ok(metadata) => metadata,
                            };

                            let has_correct_mode = metadata.permissions().mode() == entry.mode();
                            let mut has_correct_owner = metadata.uid() == geteuid().as_raw();

                            // Can we still share this payload if it doesn't
                            // have the correct owner?
                            if has_correct_mode && !has_correct_owner && {
                                // require that a file has the "other" read bit
                                // enabled before sharing it with other users.
                                (entry.mode() & 0o004) != 0
                            } {
                                if let Ok(config) = get_config() {
                                    if config.storage.allow_payload_sharing_between_users {
                                        has_correct_owner = true;
                                    }
                                }
                            }

                            if has_correct_mode && has_correct_owner {
                                // This still creates the proxy "hop" to the
                                // real payload file. It helps keep the code
                                // simple and could be a debugging aid if
                                // a proxy file is discovered to have the
                                // wrong owner/permissions then we know there
                                // is a bug somewhere.
                                if let Err(err) =
                                    tokio::fs::hard_link(&payload_path, &proxy_path).await
                                {
                                    match err.kind() {
                                        std::io::ErrorKind::NotFound if retry_count < 3 => {
                                            // At least on xfs filesystems, it
                                            // is observed that this hard_link
                                            // can fail if another process has
                                            // renamed a different file on top
                                            // of the source file. Confusingly,
                                            // `payload_path_exists` is true in
                                            // this situation, despite the "not
                                            // found" error.
                                            retry_count += 1;
                                            continue;
                                        }
                                        std::io::ErrorKind::AlreadyExists => {
                                            RenderBlobResult::PayloadAlreadyExists
                                        }
                                        _ if err.os_error()
                                            == Some(nix::errno::Errno::EMLINK as i32) =>
                                        {
                                            // hard-linking can fail if we have reached the maximum number of links
                                            // for the underlying file system. Often this number is arbitrarily large,
                                            // but on some systems and filers, or at certain scales the possibility is
                                            // very real. In these cases, our only real course of action other than failing
                                            // is to fall back to a real copy of the file.
                                            self.render_blob_with_permit(
                                                target_dir_fd,
                                                entry,
                                                RenderType::Copy,
                                                permit,
                                            )
                                            .await?;
                                            return Ok(RenderBlobResult::PayloadCopiedLinkLimit);
                                        }
                                        _ => {
                                            return Err(Error::StorageWriteError(
                                                "hard_link of payload to proxy path",
                                                proxy_path,
                                                err,
                                            ));
                                        }
                                    }
                                } else {
                                    // Reset the retry counter after this phase
                                    // so the next retryable section gets a
                                    // fair number of retries too.
                                    retry_count = 0;
                                    RenderBlobResult::PayloadHardLinked
                                }
                            } else {
                                if !has_correct_mode {
                                    tracing::debug!(actual_mode = ?metadata.permissions().mode(), expected_mode = ?entry.mode(), ?payload_path, "couldn't skip proxy copy; payload had wrong mode");
                                } else if !has_correct_owner {
                                    tracing::debug!(actual_uid = ?metadata.uid(), expected_uid = ?geteuid().as_raw(), ?payload_path, "couldn't skip proxy copy; payload had wrong uid");
                                }

                                // Write to a temporary file so that some other render
                                // process doesn't think a partially-written file is
                                // good.
                                let temp_proxy_file = tempfile::NamedTempFile::new_in(
                                    path_to_create,
                                )
                                .map_err(|err| {
                                    Error::StorageWriteError(
                                        "create proxy temp file",
                                        path_to_create.to_owned(),
                                        err,
                                    )
                                })?;
                                let mut payload_file =
                                    tokio::fs::File::open(&payload_path).await.map_err(|err| {
                                        if err.kind() == std::io::ErrorKind::NotFound {
                                            // in the case of a corrupt repository, this is a more appropriate error
                                            Error::UnknownObject(*entry.object())
                                        } else {
                                            Error::StorageReadError(
                                                "open payload for proxying",
                                                payload_path.clone(),
                                                err,
                                            )
                                        }
                                    })?;
                                let proxy_file_fd =
                                    nix::unistd::dup(temp_proxy_file.as_file().as_raw_fd())?;
                                // Safety: from_raw_fd takes ownership of this fd which is what we want
                                let mut proxy_file =
                                    unsafe { tokio::fs::File::from_raw_fd(proxy_file_fd) };
                                tokio::io::copy(&mut payload_file, &mut proxy_file)
                                    .await
                                    .map_err(|err| {
                                        Error::StorageWriteError(
                                            "copy of blob to proxy file",
                                            temp_proxy_file.path().to_owned(),
                                            err,
                                        )
                                    })?;
                                nix::sys::stat::fchmod(
                                    proxy_file_fd,
                                    Mode::from_bits_truncate(entry.mode()),
                                )
                                .map_err(|err| {
                                    Error::StorageWriteError(
                                        "set permissions on proxy payload",
                                        PathBuf::from(entry.name()),
                                        err.into(),
                                    )
                                })?;
                                // Move temporary file into place.
                                if let Err(err) = temp_proxy_file.persist_noclobber(&proxy_path) {
                                    match err.error.kind() {
                                        std::io::ErrorKind::AlreadyExists => {
                                            RenderBlobResult::PayloadAlreadyExists
                                        }
                                        _ => {
                                            return Err(Error::StorageWriteError(
                                                "persist of blob proxy file",
                                                proxy_path.to_owned(),
                                                err.error,
                                            ));
                                        }
                                    }
                                } else if !has_correct_mode {
                                    RenderBlobResult::PayloadCopiedWrongMode
                                } else {
                                    RenderBlobResult::PayloadCopiedWrongOwner
                                }
                            }
                        } else {
                            RenderBlobResult::PayloadAlreadyExists
                        };
                        // Renders should hard link to this proxy file; it will
                        // be owned by the current user and (eventually) have the
                        // expected mode.
                        committed_path = proxy_path;

                        render_blob_result
                    } else {
                        return Err(
                            "Cannot render blob as hard link to repository with no render store"
                                .into(),
                        );
                    };

                    break if let Err(err) = nix::unistd::linkat(
                        None,
                        committed_path.as_path(),
                        Some(target_dir_fd),
                        std::path::Path::new(entry.name()),
                        nix::fcntl::AtFlags::AT_SYMLINK_FOLLOW,
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
                            nix::errno::Errno::ENOENT if !committed_path.exists() => {
                                return Err(if committed_path == payload_path {
                                    // in the case of a corrupt repository, this is a more appropriate error
                                    Error::UnknownObject(*entry.object())
                                } else {
                                    Error::StorageWriteError(
                                        "hard_link from committed path",
                                        committed_path,
                                        err.into(),
                                    )
                                });
                            }
                            nix::errno::Errno::EEXIST => RenderBlobResult::PayloadAlreadyExists,
                            nix::errno::Errno::EMLINK => {
                                // hard-linking can fail if we have reached the maximum number of links
                                // for the underlying file system. Often this number is arbitrarily large,
                                // but on some systems and filers, or at certain scales the possibility is
                                // very real. In these cases, our only real course of action other than failing
                                // is to fall back to a real copy of the file.
                                self.render_blob_with_permit(
                                    dir_fd,
                                    entry,
                                    RenderType::Copy,
                                    permit,
                                )
                                .await?;
                                RenderBlobResult::PayloadCopiedLinkLimit
                            }
                            _ if matches!(render_type, RenderType::HardLink) => {
                                return Err(Error::StorageWriteError(
                                    "hard_link of blob proxy to rendered path",
                                    PathBuf::from(entry.name()),
                                    err.into(),
                                ));
                            }
                            _ => {
                                return Err(Error::StorageWriteError(
                                    "hard_link of blob to rendered path",
                                    PathBuf::from(entry.name()),
                                    err.into(),
                                ));
                            }
                        }
                    } else {
                        render_blob_result
                    };
                }
            }
            RenderType::Copy => {
                let name = entry.name().to_owned();
                let mut payload_file =
                    tokio::fs::File::open(&committed_path)
                        .await
                        .map_err(|err| {
                            Error::StorageReadError(
                                "open of payload source file",
                                committed_path,
                                err,
                            )
                        })?;
                let mut rendered_file =
                    tokio::task::spawn_blocking(move || -> std::io::Result<tokio::fs::File> {
                        // create with open permissions, as they will be set to the proper mode in the future
                        let fd = nix::fcntl::openat(
                            Some(target_dir_fd),
                            name.as_str(),
                            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_TRUNC,
                            Mode::all(),
                        )?;
                        // Safety: from_raw_fd takes ownership of this fd which is what we want
                        Ok(unsafe { tokio::fs::File::from_raw_fd(fd) })
                    })
                    .await
                    .expect("syscall should not panic")
                    .map_err(|err| {
                        Error::StorageWriteError(
                            "creation of rendered blob file",
                            PathBuf::from(entry.name()),
                            err,
                        )
                    })?;
                tokio::io::copy(&mut payload_file, &mut rendered_file)
                    .await
                    .map_err(|err| {
                        Error::StorageWriteError(
                            "copy of blob to rendered file",
                            PathBuf::from(entry.name()),
                            err,
                        )
                    })?;
                let mode = entry.mode();
                return tokio::task::spawn_blocking(move || {
                    nix::sys::stat::fchmod(
                        rendered_file.as_raw_fd(),
                        Mode::from_bits_truncate(mode),
                    )
                })
                .await
                .expect("syscall should not panic")
                .map(|_| RenderBlobResult::PayloadCopiedByRequest)
                .map_err(|err| {
                    Error::StorageWriteError(
                        "set permissions on copied payload",
                        PathBuf::from(entry.name()),
                        err.into(),
                    )
                });
            }
        })
    }
}

async fn create_and_open_dir_at<A>(dir_fd: A, name: String) -> std::io::Result<tokio::fs::File>
where
    A: AsRawFd + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        // leave the permissions open for now, so that
        // the structure inside can be generated without
        // privileged access
        match nix::sys::stat::mkdirat(Some(dir_fd.as_raw_fd()), name.as_str(), Mode::all()) {
            Ok(_) | Err(nix::errno::Errno::EEXIST) => {}
            Err(err) => return Err(err.into()),
        }
        let fd = nix::fcntl::openat(
            Some(dir_fd.as_raw_fd()),
            name.as_str(),
            OFlag::O_DIRECTORY | OFlag::O_RDONLY,
            Mode::all(),
        )
        .map_err(std::io::Error::from)?;
        // Safety: from_raw_fd takes ownership of this fd which is what we want
        Ok(unsafe { tokio::fs::File::from_raw_fd(fd) })
    })
    .await
    .map_err(|_join_err| std::io::Error::new(std::io::ErrorKind::Other, "mkdir task panic'd"))?
}
