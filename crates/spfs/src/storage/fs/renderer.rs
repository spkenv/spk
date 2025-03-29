// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

#[cfg_attr(unix, path = "./renderer_unix.rs")]
#[cfg_attr(windows, path = "./renderer_win.rs")]
mod os;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::{Stream, TryFutureExt, TryStreamExt};
use tokio::sync::Semaphore;

use crate::prelude::*;
use crate::runtime::makedirs_with_perms;
use crate::storage::LocalRepository;
use crate::storage::fs::{
    ManifestRenderPath,
    OpenFsRepository,
    RenderReporter,
    SilentRenderReporter,
};
use crate::{Error, OsError, Result, encoding, graph, tracking};

#[cfg(test)]
#[path = "./renderer_test.rs"]
mod renderer_test;

/// The default limit for concurrent blobs when rendering manifests to disk.
/// See: [`Renderer::with_max_concurrent_blobs`]
pub const DEFAULT_MAX_CONCURRENT_BLOBS: usize = 100;

/// The default limit for concurrent branches when rendering manifests to disk.
/// See: [`Renderer::with_max_concurrent_branches`]
pub const DEFAULT_MAX_CONCURRENT_BRANCHES: usize = 5;

#[derive(Debug, Copy, Clone, strum::EnumString, strum::VariantNames, strum::IntoStaticStr)]
pub enum RenderType {
    HardLink,
    HardLinkNoProxy,
    Copy,
}

impl OpenFsRepository {
    fn get_render_storage(&self) -> Result<&crate::storage::fs::FsHashStore> {
        match &self.renders {
            Some(render_store) => Ok(&render_store.renders),
            None => Err(Error::NoRenderStorage(self.address().into_owned())),
        }
    }

    pub async fn has_rendered_manifest(&self, digest: encoding::Digest) -> bool {
        let renders = match &self.renders {
            Some(render_store) => &render_store.renders,
            None => return false,
        };
        let rendered_dir = renders.build_digest_path(&digest);
        was_render_completed(rendered_dir).await
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

    pub fn proxy_path(&self) -> Option<&std::path::Path> {
        self.fs_impl
            .renders
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
        if let Err(err) = makedirs_with_perms(&workdir, renders.directory_permissions) {
            return Err(Error::StorageWriteError(
                "create working directory",
                workdir,
                err,
            ));
        }
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
                ));
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

impl ManifestRenderPath for OpenFsRepository {
    fn manifest_render_path(&self, manifest: &graph::Manifest) -> Result<PathBuf> {
        Ok(self
            .get_render_storage()?
            .build_digest_path(&manifest.digest()?))
    }
}

/// A semaphore for limiting the concurrency of blob renders.
// Allow: .0 is never read (on Windows), but it still serves a purpose.
struct BlobSemaphore(Arc<Semaphore>);

/// A newtype to represent holding the permit specifically for the blob semaphore.
// Allow: .0 is never read, but it still serves a purpose.
#[allow(dead_code)]
struct BlobSemaphorePermit<'a>(tokio::sync::SemaphorePermit<'a>);

impl BlobSemaphore {
    /// Acquires a permit from the blob semaphore.
    ///
    /// Wrapper around [`tokio::sync::Semaphore::acquire`].
    // Allow: unused on Windows.
    #[allow(dead_code)]
    async fn acquire(&self) -> BlobSemaphorePermit<'_> {
        BlobSemaphorePermit(
            self.0
                .acquire()
                .await
                .expect("semaphore should remain open"),
        )
    }
}

/// Renders manifest data to a directory on disk
pub struct Renderer<'repo, Repo, Reporter: RenderReporter = SilentRenderReporter> {
    repo: &'repo Repo,
    // Allow: unused on Windows.
    #[allow(dead_code)]
    reporter: Arc<Reporter>,
    blob_semaphore: BlobSemaphore,
    max_concurrent_branches: usize,
}

impl<'repo, Repo> Renderer<'repo, Repo, SilentRenderReporter> {
    pub fn new(repo: &'repo Repo) -> Self {
        Self {
            repo,
            reporter: Arc::new(SilentRenderReporter),
            blob_semaphore: BlobSemaphore(Arc::new(Semaphore::new(DEFAULT_MAX_CONCURRENT_BLOBS))),
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
        self.blob_semaphore = BlobSemaphore(Arc::new(Semaphore::new(max_concurrent_blobs)));
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
    pub async fn render(
        &self,
        stack: &graph::Stack,
        render_type: Option<RenderType>,
    ) -> Result<Vec<PathBuf>> {
        let layers = crate::resolve::resolve_stack_to_layers_with_repo(stack, self.repo)
            .await
            .map_err(|err| err.wrap("resolve stack to layers"))?;
        let mut futures = futures::stream::FuturesOrdered::new();
        for layer in layers {
            if let Some(manifest_digest) = layer.manifest() {
                let digest = *manifest_digest;
                let fut = self
                    .repo
                    .read_manifest(digest)
                    .map_err(move |err| err.wrap(format!("read manifest {digest}")))
                    .and_then(move |manifest| async move {
                        self.render_manifest(&manifest, render_type)
                            .await
                            .map_err(move |err| err.wrap(format!("render manifest {digest}")))
                    });
                futures.push_back(fut);
            }
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
        let mut stack = graph::Stack::default();
        for target in env_spec.iter() {
            let target = target.to_string();
            let obj = self.repo.read_ref(target.as_str()).await?;
            stack.push(obj.digest()?);
        }
        let layers = crate::resolve::resolve_stack_to_layers_with_repo(&stack, self.repo).await?;
        let mut manifests = Vec::with_capacity(layers.len());
        for layer in layers {
            if let Some(manifest_digest) = layer.manifest() {
                manifests.push(self.repo.read_manifest(*manifest_digest).await?);
            }
        }
        let mut manifest = tracking::Manifest::default();
        for next in manifests.into_iter() {
            manifest.update(&next.to_tracking_manifest());
        }
        let manifest = manifest.to_graph_manifest();
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
        if was_render_completed(&rendered_dirpath).await {
            tracing::trace!(path = ?rendered_dirpath, "render already completed");
            return Ok(rendered_dirpath);
        }
        tracing::trace!(path = ?rendered_dirpath, "rendering manifest...");

        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dir = render_store.renders.workdir().join(uuid);
        if let Err(err) = makedirs_with_perms(&working_dir, 0o777) {
            return Err(Error::StorageWriteError(
                "render_manifest::create_workdir",
                working_dir,
                err,
            ));
        }

        self.render_manifest_into_dir(
            manifest,
            &working_dir,
            render_type.unwrap_or(RenderType::HardLink),
        )
        .await
        .map_err(|err| {
            err.wrap(format!(
                "render manifest into working dir '{}'",
                working_dir.to_string_lossy()
            ))
        })?;

        render_store.renders.ensure_base_dir(&rendered_dirpath)?;
        match tokio::fs::rename(&working_dir, &rendered_dirpath).await {
            Ok(_) => (),
            Err(err) => match err.os_error() {
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
                    ));
                }
            },
        }

        Ok(rendered_dirpath)
    }
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
        #[cfg(unix)]
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

async fn was_render_completed<P: AsRef<Path>>(render_path: P) -> bool {
    tokio::fs::try_exists(render_path).await.unwrap_or_default()
}
