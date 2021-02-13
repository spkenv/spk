use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::FSRepository;
use crate::{
    encoding::Encodable,
    graph::Manifest,
    runtime::makedirs_with_perms,
    storage::{ManifestViewer, PayloadStorage},
    tracking, Result,
};

#[cfg(test)]
#[path = "./renderer_test.rs"]
mod renderer_test;

impl ManifestViewer for FSRepository {
    /// Create a hard-linked rendering of the given file manifest.
    ///
    /// # Errors:
    /// - if any of the blobs in the manifest are not available in this repo.
    fn render_manifest(&self, manifest: &crate::graph::Manifest) -> Result<PathBuf> {
        let rendered_dirpath = self.renders.build_digest_path(&manifest.digest()?);
        if was_render_completed(&rendered_dirpath) {
            return Ok(rendered_dirpath);
        }

        self.renders.ensure_base_dir(&rendered_dirpath)?;
        makedirs_with_perms(&rendered_dirpath, 0o777)?;

        let walkable = manifest.unlock();
        let entries: Vec<_> = walkable
            .walk_abs(&rendered_dirpath.to_string_lossy())
            .collect();
        for node in entries.iter() {
            match node.entry.kind {
                tracking::EntryKind::Tree => std::fs::create_dir_all(&node.path.to_path("/"))?,
                tracking::EntryKind::Mask => continue,
                tracking::EntryKind::Blob => {
                    self.render_blob(&node.path.to_path("/"), &node.entry)?
                }
            }
        }

        for node in entries.iter().rev() {
            if node.entry.kind.is_mask() {
                continue;
            }
            if libc::S_IFLNK & node.entry.mode != 0 {
                continue;
            }
            std::fs::set_permissions(
                &node.path.to_path("/"),
                std::fs::Permissions::from_mode(node.entry.mode),
            )?
        }

        mark_render_completed(&rendered_dirpath)?;
        Ok(rendered_dirpath)
    }

    /// Remove the identified render from this storage.
    fn remove_rendered_manifest(&self, digest: &crate::encoding::Digest) -> Result<()> {
        let rendered_dirpath = self.renders.build_digest_path(&digest);
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_dirpath = self.root().join(uuid);
        if let Err(err) = std::fs::rename(&rendered_dirpath, &working_dirpath) {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err.into()),
            };
        }

        unmark_render_completed(&rendered_dirpath)?;

        fn remove_dir_all(root: &PathBuf) -> Result<()> {
            for entry in std::fs::read_dir(&root)? {
                let entry = entry?;
                let entry_path = root.join(entry.file_name());
                let file_type = entry.file_type()?;
                let _ =
                    std::fs::set_permissions(&entry_path, std::fs::Permissions::from_mode(0o777));
                if file_type.is_symlink() || file_type.is_file() {
                    std::fs::remove_file(&entry_path)?;
                }
                if file_type.is_dir() {
                    remove_dir_all(&entry_path)?;
                }
            }
            std::fs::remove_dir(&root)?;
            Ok(())
        }
        remove_dir_all(&working_dirpath)
    }
}

impl FSRepository {
    fn render_blob<P: AsRef<Path>>(&self, rendered_path: P, entry: &tracking::Entry) -> Result<()> {
        if libc::S_IFLNK & entry.mode != 0 {
            let mut reader = self.open_payload(&entry.object)?;
            let mut target = String::new();
            reader.read_to_string(&mut target)?;
            if let Err(err) = std::os::unix::fs::symlink(&target, &rendered_path) {
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(err.into()),
                }
            } else {
                Ok(())
            }
        } else {
            let committed_path = self.payloads.build_digest_path(&entry.object);
            if let Err(err) = std::fs::hard_link(&committed_path, &rendered_path) {
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(err.into()),
                }
            } else {
                Ok(())
            }
        }
    }
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

/// Copy manifest contents from one directory to another.
fn copy_manifest<P: AsRef<Path>>(manifest: &Manifest, src_root: P, dst_root: P) -> Result<()> {
    let unlocked = manifest.unlock();

    for node in unlocked.walk_up() {
        if node.entry.kind.is_mask() {
            continue;
        }
        let src_path = node.path.to_path(&src_root);
        let dst_path = node.path.to_path(&dst_root);
        let meta = src_path.symlink_metadata()?;
        if let Some(parent) = dst_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(&src_path)?;
            std::os::unix::fs::symlink(&target, &dst_path)?;
        } else if meta.is_dir() {
            std::fs::set_permissions(&dst_path, meta.permissions())?;
        } else {
            std::fs::copy(src_path, dst_path)?;
        }
    }
    Ok(())
}
