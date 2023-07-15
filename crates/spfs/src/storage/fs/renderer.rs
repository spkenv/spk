// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::prelude::MetadataExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::future::ready;
use futures::{FutureExt, Stream, StreamExt, TryFutureExt, TryStreamExt};
use nix::fcntl::OFlag;
use nix::sys::stat::Mode;
use nix::unistd::geteuid;
use rand::seq::SliceRandom;
use tokio::io::AsyncReadExt;
use tokio::sync::Semaphore;

use super::render_reporter::RenderBlobResult;
use super::{FSRepository, RenderReporter, SilentRenderReporter};
use crate::encoding::{self, Encodable};
use crate::runtime::makedirs_with_perms;
use crate::storage::prelude::*;
use crate::storage::LocalRepository;
use crate::{get_config, graph, tracking, Error, Result};

#[cfg(test)]
#[path = "./renderer_test.rs"]
mod renderer_test;

/// The default limit for concurrent blobs when rendering manifests to disk.
/// See: [`Renderer::with_max_concurrent_blobs`]
pub const DEFAULT_MAX_CONCURRENT_BLOBS: usize = 100;

/// The default limit for concurrent branches when rendering manifests to disk.
/// See: [`Renderer::with_max_concurrent_branches`]
pub const DEFAULT_MAX_CONCURRENT_BRANCHES: usize = 5;

#[derive(Debug, Copy, Clone, strum::EnumString, strum::EnumVariantNames)]
pub enum RenderType {
    HardLink,
    HardLinkNoProxy,
    Copy,
}

impl FSRepository {
    fn get_render_storage(&self) -> Result<&super::FSHashStore> {
        match &self.renders {
            Some(render_store) => Ok(&render_store.renders),
            None => Err(Error::NoRenderStorage(self.address())),
        }
    }

    pub async fn has_rendered_manifest(&self, digest: encoding::Digest) -> bool {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return false,
        };
        let rendered_dir = renders.build_digest_path(&digest);
        was_render_completed(rendered_dir)
    }

    pub fn iter_rendered_manifests<'db>(
        &'db self,
    ) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>> + Send + Sync + 'db>> {
        Box::pin(try_stream! {
            let renders = self.get_render_storage()?;
            for await digest in renders.iter() {
                yield digest?;
            }
        })
    }

    /// Return the path that the manifest would be rendered to.
    pub fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<PathBuf> {
        Ok(self
            .get_render_storage()?
            .build_digest_path(&manifest.digest()?))
    }

    pub fn proxy_path(&self) -> Option<&std::path::Path> {
        self.renders
            .as_ref()
            .map(|render_store| render_store.proxy.root())
    }

    /// Remove the identified render from this storage.
    pub async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()> {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return Ok(()),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);
        let workdir = renders.workdir();
        makedirs_with_perms(&workdir, renders.directory_permissions)?;
        Self::remove_dir_atomically(&rendered_dirpath, &workdir).await
    }

    pub(crate) async fn remove_dir_atomically(dirpath: &Path, workdir: &Path) -> Result<()> {
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dirpath = workdir.join(uuid);
        if let Err(err) = tokio::fs::rename(&dirpath, &working_dirpath).await {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(crate::Error::StorageWriteError(
                    "rename on render before removal",
                    working_dirpath,
                    err,
                )),
            };
        }

        unmark_render_completed(&dirpath).await?;
        open_perms_and_remove_all(&working_dirpath).await
    }

    /// Returns true if the render was actually removed
    pub async fn remove_rendered_manifest_if_older_than(
        &self,
        older_than: DateTime<Utc>,
        digest: encoding::Digest,
    ) -> Result<bool> {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return Ok(false),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);

        let metadata = match tokio::fs::symlink_metadata(&rendered_dirpath).await {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
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
            return Ok(false);
        }

        self.remove_rendered_manifest(digest).await?;
        Ok(true)
    }
}

/// Renders manifest data to a directory on disk
pub struct Renderer<'repo, Repo, Reporter: RenderReporter = SilentRenderReporter> {
    repo: &'repo Repo,
    reporter: Arc<Reporter>,
    blob_semaphore: Arc<Semaphore>,
    max_concurrent_branches: usize,
}

impl<'repo, Repo> Renderer<'repo, Repo, SilentRenderReporter> {
    pub fn new(repo: &'repo Repo) -> Self {
        Self {
            repo,
            reporter: Arc::new(SilentRenderReporter::default()),
            blob_semaphore: Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_BLOBS)),
            max_concurrent_branches: DEFAULT_MAX_CONCURRENT_BRANCHES,
        }
    }
}

impl<'repo, Repo, Reporter> Renderer<'repo, Repo, Reporter>
where
    Repo: Repository + LocalRepository,
    Reporter: RenderReporter,
{
    /// Report progress to the given instance, replacing any existing one
    pub fn with_reporter<T, R>(self, reporter: T) -> Renderer<'repo, Repo, R>
    where
        T: Into<Arc<R>>,
        R: RenderReporter,
    {
        Renderer {
            repo: self.repo,
            reporter: reporter.into(),
            blob_semaphore: self.blob_semaphore,
            max_concurrent_branches: self.max_concurrent_branches,
        }
    }

    /// Set how many blobs should be processed at once.
    pub fn with_max_concurrent_blobs(mut self, max_concurrent_blobs: usize) -> Self {
        self.blob_semaphore = Arc::new(Semaphore::new(max_concurrent_blobs));
        self
    }

    /// Set how many branches should be processed at once.
    ///
    /// Each tree that is processed can have any number of subtrees. This number
    /// limits the number of subtrees that can be processed at once for any given tree. This
    /// means that the number compounds exponentially based on the depth of the manifest
    /// being rendered. Eg: a limit of 2 allows two directories to be processed in the root
    /// simultaneously and a further 2 within each of those two for a total of 4 branches, and so
    /// on. When rendering extremely deep trees, a smaller, conservative number is better
    /// to avoid open file limits.
    pub fn with_max_concurrent_branches(mut self, max_concurrent_branches: usize) -> Self {
        self.max_concurrent_branches = max_concurrent_branches;
        self
    }

    /// Render all layers in the given env to the render storage of the underlying
    /// repository, returning the paths to all relevant layers in the appropriate order.
    pub async fn render<I, D>(
        &self,
        stack: I,
        render_type: Option<RenderType>,
    ) -> Result<Vec<PathBuf>>
    where
        I: Iterator<Item = D> + Send,
        D: AsRef<encoding::Digest> + Send,
    {
        let layers = crate::resolve::resolve_stack_to_layers_with_repo(stack, self.repo).await?;
        let mut futures = futures::stream::FuturesOrdered::new();
        for layer in layers {
            let fut = self
                .repo
                .read_manifest(layer.manifest)
                .and_then(
                    |manifest| async move { self.render_manifest(&manifest, render_type).await },
                );
            futures.push_back(fut);
        }
        futures.try_collect().await
    }

    /// Recreate the full structure of a stored environment on disk
    pub async fn render_into_directory<E: Into<tracking::EnvSpec>, P: AsRef<Path>>(
        &self,
        env_spec: E,
        target_dir: P,
        render_type: RenderType,
    ) -> Result<()> {
        let env_spec = env_spec.into();
        let mut stack = Vec::new();
        for target in env_spec.iter() {
            let target = target.to_string();
            let obj = self.repo.read_ref(target.as_str()).await?;
            stack.push(obj.digest()?);
        }
        let layers =
            crate::resolve::resolve_stack_to_layers_with_repo(stack.iter(), self.repo).await?;
        let mut manifests = Vec::with_capacity(layers.len());
        for layer in layers {
            manifests.push(self.repo.read_manifest(layer.manifest).await?);
        }
        let mut manifest = tracking::Manifest::default();
        for next in manifests.into_iter() {
            manifest.update(&next.to_tracking_manifest());
        }
        let manifest = graph::Manifest::from(&manifest);
        self.render_manifest_into_dir(&manifest, target_dir, render_type)
            .await
    }

    /// Render a manifest into the renders area of the underlying repository,
    /// returning the absolute local path of the directory.
    pub async fn render_manifest(
        &self,
        manifest: &graph::Manifest,
        render_type: Option<RenderType>,
    ) -> Result<PathBuf> {
        let render_store = self.repo.render_store()?;
        let rendered_dirpath = render_store.renders.build_digest_path(&manifest.digest()?);
        if was_render_completed(&rendered_dirpath) {
            tracing::trace!(path = ?rendered_dirpath, "render already completed");
            return Ok(rendered_dirpath);
        }
        tracing::trace!(path = ?rendered_dirpath, "rendering manifest...");

        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dir = render_store.renders.workdir().join(uuid);
        makedirs_with_perms(&working_dir, 0o777)?;

        self.render_manifest_into_dir(
            manifest,
            &working_dir,
            render_type.unwrap_or(RenderType::HardLink),
        )
        .await?;

        render_store.renders.ensure_base_dir(&rendered_dirpath)?;
        match tokio::fs::rename(&working_dir, &rendered_dirpath).await {
            Ok(_) => (),
            Err(err) => match err.raw_os_error() {
                // XXX: Replace with ErrorKind::DirectoryNotEmpty when
                // stabilized.
                Some(libc::EEXIST) | Some(libc::ENOTEMPTY) => {
                    // The rename failed because the destination
                    // `rendered_dirpath` exists and is a non-empty directory.
                    // Assume we lost a race with some other process rendering
                    // the same manifest. Treat this as a success, but clean up
                    // the working directory left behind.
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

        let mut res = self
            .render_into_dir_fd(root_dir, root_node.clone(), manifest, render_type)
            .await;
        if let Err(Error::StorageWriteError(_, p, _)) = &mut res {
            *p = target_dir.join(p.as_path());
        }
        res?;
        self.reporter.rendered_layer(manifest);
        Ok(())
    }

    #[async_recursion::async_recursion]
    pub async fn render_into_dir_fd<Fd>(
        &self,
        root_dir_fd: Fd,
        tree: graph::Tree,
        manifest: &graph::Manifest,
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
        let mut entries = tree.entries.into_iter().collect::<Vec<_>>();
        entries.shuffle(&mut rand::thread_rng());

        let root_dir_fd = root_dir_fd.as_raw_fd();
        let mut stream = futures::stream::iter(entries)
            .then(move |entry| {
                self.reporter.visit_entry(&entry);
                let fut = async move {
                    let mut root_path = PathBuf::from(&entry.name);
                    match entry.kind {
                        tracking::EntryKind::Tree => {
                            let tree = manifest.get_tree(&entry.object).ok_or_else(|| {
                                Error::String(format!("Failed to render: manifest is internally inconsistent (missing child tree {})", entry.object))
                            })?;

                            let child_dir = create_and_open_dir_at(root_dir_fd, entry.name.clone())
                                .await
                                .map_err(|err| {
                                    Error::StorageWriteError(
                                        "create dir during render",
                                        PathBuf::from(&entry.name),
                                        err,
                                    )
                                })?;
                            let mut res = self
                                .render_into_dir_fd(
                                    child_dir.as_raw_fd(),
                                    tree.clone(),
                                    manifest,
                                    render_type,
                                )
                                .await;
                            if res.is_ok() {
                                res = tokio::task::spawn_blocking(move || {
                                    nix::sys::stat::fchmod(
                                        child_dir.as_raw_fd(),
                                        Mode::from_bits_truncate(entry.mode),
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
                            res.map(|_| None)
                        }
                        tracking::EntryKind::Mask => Ok(None),
                        tracking::EntryKind::Blob => {
                            self.render_blob(root_dir_fd, &entry, render_type).await.map(Some)
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
                    self.reporter.rendered_blob(&entry, &render_blob_result);
                    self.reporter.rendered_entry(&entry);
                }
                Ok((entry, _)) => self.reporter.rendered_entry(&entry),
            }
        }

        Ok(())
    }
    /// Renders the file into a path on disk, changing its permissions
    /// as necessary / appropriate
    #[async_recursion::async_recursion]
    async fn render_blob<'a, Fd>(
        &self,
        dir_fd: Fd,
        entry: &graph::Entry,
        render_type: RenderType,
    ) -> Result<RenderBlobResult>
    where
        Fd: std::os::fd::AsRawFd + Send,
    {
        let _permit = self
            .blob_semaphore
            .acquire()
            .await
            .expect("semaphore should remain open");
        // Note that opening the payload, even if the return value is not
        // used, has a possible side effect of repairing a missing payload,
        // depending on repository implementation.
        // When the blob is not a symlink, the code below will try to access
        // the payload file without calling `open_payload`. If `open_payload`
        // is not called here, the non-symlink code may fail due to a missing
        // a payload that could have been repaired.
        let (mut reader, filename) = self.repo.open_payload(entry.object).await?;
        let target_dir_fd = dir_fd.as_raw_fd();
        if entry.is_symlink() {
            let mut target = String::new();
            {
                reader.read_to_string(&mut target).await.map_err(|err| {
                    Error::StorageReadError("read_to_string on render blob", filename, err)
                })?;
            }
            return if let Err(err) =
                nix::unistd::symlinkat(target.as_str(), Some(target_dir_fd), entry.name.as_str())
            {
                match err {
                    nix::errno::Errno::EEXIST => Ok(RenderBlobResult::SymlinkAlreadyExists),
                    _ => Err(Error::StorageWriteError(
                        "symlink on rendered blob",
                        PathBuf::from(&entry.name),
                        err.into(),
                    )),
                }
            } else {
                Ok(RenderBlobResult::SymlinkWritten)
            };
        }
        // Free up file resources as early as possible.
        drop(reader);

        let mut committed_path = self.repo.payloads().build_digest_path(&entry.object);
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
                            .build_digest_path(&entry.object)
                            .join(entry.mode.to_string());
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
                                    ))
                                }
                                Ok(metadata) => metadata,
                            };

                            let has_correct_mode = metadata.permissions().mode() == entry.mode;
                            let mut has_correct_owner = metadata.uid() == geteuid().as_raw();

                            // Can we still share this payload if it doesn't
                            // have the correct owner?
                            if has_correct_mode && !has_correct_owner && {
                                // require that a file has the "other" read bit
                                // enabled before sharing it with other users.
                                (entry.mode & 0o004) != 0
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
                                        _ if err.raw_os_error()
                                            == Some(nix::errno::Errno::EMLINK as i32) =>
                                        {
                                            // hard-linking can fail if we have reached the maximum number of links
                                            // for the underlying file system. Often this number is arbitrarily large,
                                            // but on some systems and filers, or at certain scales the possibility is
                                            // very real. In these cases, our only real course of action other than failing
                                            // is to fall back to a real copy of the file.
                                            let _ = self
                                                .render_blob(target_dir_fd, entry, RenderType::Copy)
                                                .await?;
                                            return Ok(RenderBlobResult::PayloadCopiedLinkLimit);
                                        }
                                        _ => {
                                            return Err(Error::StorageWriteError(
                                                "hard_link of payload to proxy path",
                                                proxy_path,
                                                err,
                                            ))
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
                                    tracing::debug!(actual_mode = ?metadata.permissions().mode(), expected_mode = ?entry.mode, ?payload_path, "couldn't skip proxy copy; payload had wrong mode");
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
                                            // in the case of a corrupt repository, this is a more appropate error
                                            Error::UnknownObject(entry.object)
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
                                        std::io::ErrorKind::AlreadyExists => {
                                            RenderBlobResult::PayloadAlreadyExists
                                        }
                                        _ => {
                                            return Err(Error::StorageWriteError(
                                                "persist of blob proxy file",
                                                proxy_path.to_owned(),
                                                err.error,
                                            ))
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
                            nix::errno::Errno::ENOENT if !committed_path.exists() => {
                                return Err(if committed_path == payload_path {
                                    // in the case of a corrupt repository, this is a more appropate error
                                    Error::UnknownObject(entry.object)
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
                                let _ = self.render_blob(dir_fd, entry, RenderType::Copy).await?;
                                RenderBlobResult::PayloadCopiedLinkLimit
                            }
                            _ if matches!(render_type, RenderType::HardLink) => {
                                return Err(Error::StorageWriteError(
                                    "hard_link of blob proxy to rendered path",
                                    PathBuf::from(&entry.name),
                                    err.into(),
                                ))
                            }
                            _ => {
                                return Err(Error::StorageWriteError(
                                    "hard_link of blob to rendered path",
                                    PathBuf::from(&entry.name),
                                    err.into(),
                                ))
                            }
                        }
                    } else {
                        render_blob_result
                    };
                }
            }
            RenderType::Copy => {
                let name = entry.name.clone();
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
                            target_dir_fd,
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
                            PathBuf::from(&entry.name),
                            err,
                        )
                    })?;
                tokio::io::copy(&mut payload_file, &mut rendered_file)
                    .await
                    .map_err(|err| {
                        Error::StorageWriteError(
                            "copy of blob to rendered file",
                            PathBuf::from(&entry.name),
                            err,
                        )
                    })?;
                let mode = entry.mode;
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
                        PathBuf::from(&entry.name),
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
        match nix::sys::stat::mkdirat(dir_fd.as_raw_fd(), name.as_str(), Mode::all()) {
            Ok(_) | Err(nix::errno::Errno::EEXIST) => {}
            Err(err) => return Err(err.into()),
        }
        let fd = nix::fcntl::openat(
            dir_fd.as_raw_fd(),
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

/// Walks down a filesystem tree, opening permissions on each file before removing
/// the entire tree.
///
/// This process handles the case when a folder may include files
/// that need to be removed but on which the user doesn't have enough permissions.
/// It does assume that the current user owns the file, as it may not be possible to
/// change permissions before removal otherwise.
#[async_recursion::async_recursion]
pub async fn open_perms_and_remove_all(root: &Path) -> Result<()> {
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
