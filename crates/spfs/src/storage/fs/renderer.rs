// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::FSRepository;
use crate::{
    encoding::{self, Encodable},
    runtime::makedirs_with_perms,
    storage::{ManifestViewer, PayloadStorage, Repository},
    tracking, Error, Result,
};

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
            Some(renders) => renders,
            None => return false,
        };
        let rendered_dir = renders.build_digest_path(&digest);
        was_render_completed(&rendered_dir)
    }

    /// Create a hard-linked rendering of the given file manifest.
    ///
    /// # Errors:
    /// - if any of the blobs in the manifest are not available in this repo.
    async fn render_manifest(&self, manifest: &crate::graph::Manifest) -> Result<PathBuf> {
        let renders = match &self.renders {
            Some(renders) => renders,
            None => return Err(Error::NoRenderStorage(self.address())),
        };
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
        match std::fs::rename(&working_dir, &rendered_dirpath) {
            Ok(_) => (),
            Err(err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    if let Err(err) = open_perms_and_remove_all(&working_dir) {
                        tracing::warn!(path=?working_dir, "failed to clean up working directory: {:?}", err);
                    }
                }
                _ => return Err(Error::wrap_io(err, "Failed to finalize render")),
            },
        }

        mark_render_completed(&rendered_dirpath)?;
        Ok(rendered_dirpath)
    }

    /// Remove the identified render from this storage.
    async fn remove_rendered_manifest(&self, digest: crate::encoding::Digest) -> Result<()> {
        let renders = match &self.renders {
            Some(renders) => renders,
            None => return Ok(()),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dirpath = renders.workdir().join(uuid);
        renders.ensure_base_dir(&working_dirpath)?;
        if let Err(err) = std::fs::rename(&rendered_dirpath, &working_dirpath) {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(crate::Error::wrap_io(
                    err,
                    "Failed to pull render for deletion",
                )),
            };
        }

        unmark_render_completed(&rendered_dirpath)?;
        open_perms_and_remove_all(&working_dirpath)
    }
}

impl FSRepository {
    pub async fn render_manifest_into_dir(
        &self,
        manifest: &crate::graph::Manifest,
        target_dir: impl AsRef<Path>,
        render_type: RenderType,
    ) -> Result<()> {
        let walkable = manifest.unlock();
        let entries: Vec<_> = walkable
            .walk_abs(&target_dir.as_ref().to_string_lossy())
            .collect();
        // we used to get CAP_FOWNER here, but with async
        // it can no longer garentee anything useful
        // (the process can happen in other threads, and
        // other code can run in the current thread)
        for node in entries.iter() {
            let res = match node.entry.kind {
                tracking::EntryKind::Tree => {
                    std::fs::create_dir_all(&node.path.to_path("/")).map_err(|e| e.into())
                }
                tracking::EntryKind::Mask => continue,
                tracking::EntryKind::Blob => {
                    self.render_blob(node.path.to_path("/"), node.entry, &render_type)
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
            if let Err(err) = std::fs::set_permissions(
                &node.path.to_path("/"),
                std::fs::Permissions::from_mode(node.entry.mode),
            ) {
                return Err(Error::wrap_io(
                    err,
                    format!("Failed to set permissions [{}]", node.path),
                ));
            }
        }

        Ok(())
    }

    async fn render_blob<P: AsRef<Path>>(
        &self,
        rendered_path: P,
        entry: &tracking::Entry,
        render_type: &RenderType,
    ) -> Result<()> {
        if entry.is_symlink() {
            let mut reader = self.open_payload(entry.object).await?;
            let mut target = String::new();
            reader.read_to_string(&mut target)?;
            return if let Err(err) = std::os::unix::fs::symlink(&target, &rendered_path) {
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(Error::wrap_io(err, "Failed to render symlink")),
                }
            } else {
                Ok(())
            };
        }
        let committed_path = self.payloads.build_digest_path(&entry.object);
        match render_type {
            RenderType::HardLink => {
                if let Err(err) = std::fs::hard_link(&committed_path, &rendered_path) {
                    match err.kind() {
                        std::io::ErrorKind::AlreadyExists => (),
                        _ => {
                            return Err(Error::wrap_io(
                                err,
                                format!(
                                    "Failed to hardlink original:[{}] link:[{}]",
                                    committed_path.display(),
                                    rendered_path.as_ref().display()
                                ),
                            ))
                        }
                    }
                }
            }
            RenderType::Copy => {
                if let Err(err) = std::fs::copy(&committed_path, &rendered_path) {
                    match err.kind() {
                        std::io::ErrorKind::AlreadyExists => (),
                        _ => return Err(Error::wrap_io(err, "Failed to copy file")),
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
fn open_perms_and_remove_all(root: &Path) -> Result<()> {
    for entry in std::fs::read_dir(&root)? {
        let entry = entry?;
        let entry_path = root.join(entry.file_name());
        let file_type = entry.file_type()?;
        let _ = std::fs::set_permissions(&entry_path, std::fs::Permissions::from_mode(0o777));
        if file_type.is_symlink() || file_type.is_file() {
            std::fs::remove_file(&entry_path)?;
        }
        if file_type.is_dir() {
            open_perms_and_remove_all(&entry_path)?;
        }
    }
    std::fs::remove_dir(&root)?;
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
fn mark_render_completed<P: AsRef<Path>>(render_path: P) -> Result<()> {
    let mut name = render_path
        .as_ref()
        .file_name()
        .expect("must have a file name")
        .to_os_string();
    name.push(".completed");
    let marker_path = render_path.as_ref().with_file_name(name);
    // create if it doesn't exist but don't fail if it already exists (no exclusive open)
    std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&marker_path)?;
    Ok(())
}

fn unmark_render_completed<P: AsRef<Path>>(render_path: P) -> Result<()> {
    let mut name = render_path
        .as_ref()
        .file_name()
        .expect("must have a file name")
        .to_os_string();
    name.push(".completed");
    let marker_path = render_path.as_ref().with_file_name(name);
    if let Err(err) = std::fs::remove_file(&marker_path) {
        match err.kind() {
            std::io::ErrorKind::NotFound => Ok(()),
            _ => Err(err.into()),
        }
    } else {
        Ok(())
    }
}
