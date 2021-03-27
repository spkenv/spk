use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::FSRepository;
use crate::{
    encoding::{self, Encodable},
    runtime::makedirs_with_perms,
    storage::{ManifestViewer, PayloadStorage},
    tracking, Result,
};

#[cfg(test)]
#[path = "./renderer_test.rs"]
mod renderer_test;

pub enum RenderType {
    HardLink,
    Copy,
}

impl ManifestViewer for FSRepository {
    fn has_rendered_manifest(&self, digest: &encoding::Digest) -> bool {
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
    fn render_manifest(&self, manifest: &crate::graph::Manifest) -> Result<PathBuf> {
        let renders = match &self.renders {
            Some(renders) => renders,
            None => return Err("repository has not been setup for rendering manifests".into()),
        };
        let rendered_dirpath = renders.build_digest_path(&manifest.digest()?);
        if was_render_completed(&rendered_dirpath) {
            tracing::trace!(path = ?rendered_dirpath, "render already completed");
            return Ok(rendered_dirpath);
        }
        tracing::trace!(path = ?rendered_dirpath, "rendering manifest...");

        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dir = renders.root().join(uuid);
        makedirs_with_perms(&working_dir, 0o777)?;

        self.render_manifest_into_dir(&manifest, &working_dir, RenderType::HardLink)?;

        renders.ensure_base_dir(&rendered_dirpath)?;
        match std::fs::rename(&working_dir, &rendered_dirpath) {
            Ok(_) => (),
            Err(err) => match err.kind() {
                std::io::ErrorKind::AlreadyExists => {
                    if let Err(err) = open_perms_and_remove_all(&working_dir) {
                        tracing::warn!(path=?working_dir, "failed to clean up working directory: {:?}", err);
                    }
                }
                _ => return Err(err.into()),
            },
        }

        mark_render_completed(&rendered_dirpath)?;
        Ok(rendered_dirpath)
    }

    /// Remove the identified render from this storage.
    fn remove_rendered_manifest(&self, digest: &crate::encoding::Digest) -> Result<()> {
        let renders = match &self.renders {
            Some(renders) => renders,
            None => return Ok(()),
        };
        let rendered_dirpath = renders.build_digest_path(&digest);
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dirpath = self.root().join(uuid);
        if let Err(err) = std::fs::rename(&rendered_dirpath, &working_dirpath) {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err.into()),
            };
        }

        unmark_render_completed(&rendered_dirpath)?;
        open_perms_and_remove_all(&working_dirpath)
    }
}

impl FSRepository {
    pub fn render_manifest_into_dir(
        &self,
        manifest: &crate::graph::Manifest,
        target_dir: impl AsRef<Path>,
        render_type: RenderType,
    ) -> Result<()> {
        let walkable = manifest.unlock();
        let entries: Vec<_> = walkable
            .walk_abs(&target_dir.as_ref().to_string_lossy())
            .collect();
        for node in entries.iter() {
            match node.entry.kind {
                tracking::EntryKind::Tree => std::fs::create_dir_all(&node.path.to_path("/"))?,
                tracking::EntryKind::Mask => continue,
                tracking::EntryKind::Blob => {
                    self.render_blob(node.path.to_path("/"), &node.entry, &render_type)?
                }
            }
        }

        for node in entries.iter().rev() {
            if node.entry.kind.is_mask() {
                continue;
            }
            if node.entry.is_symlink() {
                continue;
            }
            std::fs::set_permissions(
                &node.path.to_path("/"),
                std::fs::Permissions::from_mode(node.entry.mode),
            )?;
        }
        Ok(())
    }

    fn render_blob<P: AsRef<Path>>(
        &self,
        rendered_path: P,
        entry: &tracking::Entry,
        render_type: &RenderType,
    ) -> Result<()> {
        if entry.is_symlink() {
            let mut reader = self.open_payload(&entry.object)?;
            let mut target = String::new();
            reader.read_to_string(&mut target)?;
            return if let Err(err) = std::os::unix::fs::symlink(&target, &rendered_path) {
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(crate::Error::new_errno(
                        err.raw_os_error().unwrap_or(libc::EINVAL),
                        format!(
                            "Failed to hardlink {{{:?} => {:?}}}: {:?}",
                            &target,
                            &rendered_path.as_ref(),
                            err,
                        ),
                    )),
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
                        _ => return Err(err.into()),
                    }
                }
            }
            RenderType::Copy => {
                std::fs::copy(&committed_path, &rendered_path)?;
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
fn open_perms_and_remove_all(root: &PathBuf) -> Result<()> {
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
        .create_new(true)
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
