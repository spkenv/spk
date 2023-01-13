// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use async_stream::try_stream;
use chrono::{DateTime, Utc};
use futures::Stream;
use tokio::io::AsyncReadExt;

use super::FSRepository;
use crate::encoding::{self, Encodable};
use crate::runtime::makedirs_with_perms;
use crate::storage::{self, ManifestViewer, PayloadStorage, Repository};
use crate::{graph, tracking, Error, Result};

#[cfg(test)]
#[path = "./renderer_test.rs"]
mod renderer_test;

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
    fn manifest_render_path(&self, manifest: &crate::graph::Manifest) -> Result<PathBuf> {
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
    async fn render_manifest(
        &self,
        manifest: &crate::graph::Manifest,
        pull_from: Option<&storage::RepositoryHandle>,
    ) -> Result<PathBuf> {
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

        self.render_manifest_into_dir(manifest, &working_dir, RenderType::HardLink, pull_from)
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

    pub async fn render_manifest_into_dir(
        &self,
        manifest: &crate::graph::Manifest,
        target_dir: impl AsRef<Path>,
        render_type: RenderType,
        pull_from: Option<&storage::RepositoryHandle>,
    ) -> Result<()> {
        let walkable = manifest.unlock();
        let entries: Vec<_> = walkable
            .walk_abs(&target_dir.as_ref().to_string_lossy())
            .collect();
        // we used to get CAP_FOWNER here, but with async
        // it can no longer guarantee anything useful
        // (the process can happen in other threads, and
        // other code can run in the current thread)
        for node in entries.iter() {
            let res = match node.entry.kind {
                tracking::EntryKind::Tree => {
                    let path_to_create = node.path.to_path("/");
                    tokio::fs::create_dir_all(&path_to_create)
                        .await
                        .map_err(|err| {
                            Error::StorageWriteError(
                                "create_dir_all on render node base",
                                path_to_create,
                                err,
                            )
                        })
                }
                tracking::EntryKind::Mask => continue,
                tracking::EntryKind::Blob => {
                    self.render_blob(node.path.to_path("/"), node.entry, &render_type, pull_from)
                        .await
                }
            };
            if let Err(err) = res {
                return Err(err.wrap(format!("Failed to render [{}]", node.path)));
            }
        }

        for node in entries.iter().rev() {
            if node.entry.kind.is_mask() {
                continue;
            }
            if node.entry.is_symlink() {
                continue;
            }
            let path_to_change = node.path.to_path("/");
            if let Err(err) = tokio::fs::set_permissions(
                &path_to_change,
                std::fs::Permissions::from_mode(node.entry.mode),
            )
            .await
            {
                return Err(Error::StorageWriteError(
                    "set_permissions on render node",
                    path_to_change,
                    err,
                ));
            }
        }

        Ok(())
    }

    /// Attempt to repair a missing payload by pulling it from the given
    /// remote repository.
    ///
    /// Return true if the payload was successfully repaired.
    async fn repair_missing_payload(
        &self,
        entry: &tracking::Entry,
        pull_from: Option<&storage::RepositoryHandle>,
    ) -> bool {
        if let Some(pull_from) = pull_from {
            let dest_repo = self.clone().into();

            let syncer = crate::Syncer::new(pull_from, &dest_repo)
                .with_policy(crate::sync::SyncPolicy::ResyncEverything)
                .with_reporter(
                    // There is already a progress bar in use in this context,
                    // so don't make another one here.
                    crate::sync::SilentSyncReporter::default(),
                );
            match syncer.sync_digest(entry.object).await {
                Ok(_) => {
                    tracing::info!(
                        "Repaired a missing payload! {digest}",
                        digest = entry.object
                    );
                    #[cfg(feature = "sentry")]
                    tracing::error!(target: "sentry", object = %entry.object, "Repaired a missing payload!");
                    return true;
                }
                Err(err) => {
                    tracing::warn!("Could not repair a missing payload: {err}");
                    #[cfg(feature = "sentry")]
                    tracing::error!(
                        target: "sentry",
                        object = %entry.object,
                        ?err,
                        "Could not repair a missing payload"
                    );
                }
            }
        }

        false
    }

    async fn render_blob<P: AsRef<Path>>(
        &self,
        rendered_path: P,
        entry: &tracking::Entry,
        render_type: &RenderType,
        pull_from: Option<&storage::RepositoryHandle>,
    ) -> Result<()> {
        if entry.is_symlink() {
            let (mut reader, filename) = loop {
                match self.open_payload(entry.object).await {
                    Ok(payload) => break payload,
                    Err(err @ Error::ObjectMissingPayload(_, _)) => {
                        if self.repair_missing_payload(entry, pull_from).await {
                            continue;
                        } else {
                            return Err(err);
                        }
                    }
                    Err(err) => return Err(err),
                };
            };
            let mut target = String::new();
            reader.read_to_string(&mut target).await.map_err(|err| {
                Error::StorageReadError("read_to_string on render blob", filename, err)
            })?;
            return if let Err(err) = std::os::unix::fs::symlink(&target, &rendered_path) {
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(Error::StorageWriteError(
                        "symlink on rendered blob",
                        rendered_path.as_ref().to_owned(),
                        err,
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
                            if let Err(err) = tokio::fs::copy(&payload_path, &temp_proxy_file).await
                            {
                                match err.kind() {
                                    std::io::ErrorKind::NotFound
                                        if matches!(payload_path.try_exists(), Ok(false)) =>
                                    {
                                        if self.repair_missing_payload(entry, pull_from).await {
                                            continue;
                                        } else {
                                            return Err(Error::ObjectMissingPayload(
                                                graph::Object::Blob(graph::Blob::new(
                                                    entry.object,
                                                    entry.size,
                                                )),
                                                entry.object,
                                            ));
                                        }
                                    }
                                    _ => {
                                        return Err(Error::StorageWriteError(
                                            "copy of blob to proxy file",
                                            temp_proxy_file.path().to_owned(),
                                            err,
                                        ));
                                    }
                                }
                            }
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

                    if let Err(err) = tokio::fs::hard_link(&committed_path, &rendered_path).await {
                        match err.kind() {
                            std::io::ErrorKind::NotFound if retry_count < 3 => {
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
                            std::io::ErrorKind::AlreadyExists => (),
                            _ => {
                                return Err(Error::StorageWriteError(
                                    "hard_link of blob proxy to rendered path",
                                    rendered_path.as_ref().to_owned(),
                                    err,
                                ))
                            }
                        }
                    }

                    break;
                }
            }
            RenderType::Copy => {
                let mut retry_count = 0;
                loop {
                    let Err(err) = tokio::fs::copy(&committed_path, &rendered_path).await else {
                        break;
                    };
                    match err.kind() {
                        std::io::ErrorKind::AlreadyExists => break,
                        std::io::ErrorKind::NotFound if retry_count < 3 => {
                            // in some (NFS) environments, the thread that we
                            // are running in might not see the paths that we are
                            // copying into because they were just created. The
                            // `open` syscall is the only one that forces NFS to
                            // fetch new data from the server, and can force
                            // the client to see that the parent directory exists
                            let _ = tokio::fs::File::open(&rendered_path).await;
                            retry_count += 1;
                        }
                        std::io::ErrorKind::NotFound => {
                            // in these cases it's more likely the committed path
                            // that was the issue
                            return Err(Error::StorageWriteError(
                                "copy of blob to rendered path (showing from)",
                                committed_path,
                                err,
                            ));
                        }
                        _ => {
                            return Err(Error::StorageWriteError(
                                "copy of blob to rendered path (showing to)",
                                rendered_path.as_ref().to_owned(),
                                err,
                            ))
                        }
                    }
                }
            }
        }
        Ok(())
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
