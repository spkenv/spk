// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Scratch directory management for copy-on-write semantics on macOS.
//!
//! This module provides a [`ScratchDir`] that manages a temporary directory
//! for storing modified files in an editable SPFS mount. It implements
//! copy-on-write semantics in userspace, since macOS doesn't have overlayfs.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │    macFUSE + Userspace COW          │
//! │  ┌─────────────────────────────┐    │
//! │  │ scratch: /tmp/spfs-scratch/ │    │  ← Modified/new files
//! │  ├─────────────────────────────┤    │
//! │  │ whiteouts: HashSet<path>    │    │  ← Track deletions
//! │  ├─────────────────────────────┤    │
//! │  │ base: SPFS repos            │    │  ← Read from repos
//! │  └─────────────────────────────┘    │
//! └─────────────────────────────────────┘
//! ```

use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Error type for scratch directory operations.
#[derive(Debug, thiserror::Error)]
pub enum ScratchError {
    /// I/O error during scratch operation
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Path manipulation error
    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Manages the scratch directory for an editable mount.
///
/// The scratch directory stores:
/// - Modified files (copy-on-write from repo)
/// - Newly created files
/// - Whiteouts (tracking deleted files)
#[derive(Debug)]
pub struct ScratchDir {
    /// Root path of the scratch directory
    root: PathBuf,
    /// Set of virtual paths that have been deleted (whiteouts)
    whiteouts: RwLock<HashSet<PathBuf>>,
    /// Set of virtual paths that exist in scratch (for quick lookup)
    modified: RwLock<HashSet<PathBuf>>,
}

impl ScratchDir {
    /// Create a new scratch directory for the given runtime.
    ///
    /// The directory is created under the macOS cache directory
    /// (~/Library/Caches/spfs/scratch/) with a name based on the runtime name.
    pub fn new(runtime_name: &str) -> Result<Self, ScratchError> {
        // Use macOS-approved cache directory instead of /tmp
        let root = dirs::cache_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("spfs")
            .join("scratch")
            .join(runtime_name);
        std::fs::create_dir_all(&root)?;

        tracing::debug!(path = %root.display(), "Created scratch directory");

        Ok(Self {
            root,
            whiteouts: RwLock::new(HashSet::new()),
            modified: RwLock::new(HashSet::new()),
        })
    }

    /// Create a scratch directory at a specific path.
    ///
    /// Useful for testing or when a specific location is required.
    pub fn at_path(root: PathBuf) -> Result<Self, ScratchError> {
        std::fs::create_dir_all(&root)?;

        Ok(Self {
            root,
            whiteouts: RwLock::new(HashSet::new()),
            modified: RwLock::new(HashSet::new()),
        })
    }

    /// Get the scratch directory root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Convert a virtual path (like `/bin/foo`) to a scratch path.
    ///
    /// The virtual path's leading slash is stripped to create a relative
    /// path under the scratch root.
    pub fn scratch_path(&self, virtual_path: &Path) -> PathBuf {
        self.root
            .join(virtual_path.strip_prefix("/").unwrap_or(virtual_path))
    }

    /// Check if a virtual path is marked as deleted (whiteout).
    pub fn is_deleted(&self, virtual_path: &Path) -> bool {
        let whiteouts = self.whiteouts.read().expect("whiteouts lock");
        whiteouts.contains(virtual_path)
    }

    /// Check if a virtual path exists in the scratch directory.
    pub fn is_in_scratch(&self, virtual_path: &Path) -> bool {
        let modified = self.modified.read().expect("modified lock");
        modified.contains(virtual_path)
    }

    /// Check if a path exists in scratch by checking the filesystem.
    ///
    /// This is slower than `is_in_scratch` but doesn't require tracking.
    pub fn exists_in_scratch(&self, virtual_path: &Path) -> bool {
        self.scratch_path(virtual_path).exists()
    }

    /// Mark a virtual path as deleted (whiteout).
    ///
    /// This makes the file invisible even if it exists in the base layer.
    /// If the file exists in scratch, it is also removed.
    pub fn mark_deleted(&self, virtual_path: &Path) -> Result<(), ScratchError> {
        // Remove from scratch filesystem if present
        let scratch_path = self.scratch_path(virtual_path);
        if scratch_path.exists() {
            if scratch_path.is_dir() {
                std::fs::remove_dir_all(&scratch_path)?;
            } else {
                std::fs::remove_file(&scratch_path)?;
            }
        }

        // Add to whiteouts
        let mut whiteouts = self.whiteouts.write().expect("whiteouts lock");
        whiteouts.insert(virtual_path.to_path_buf());

        // Remove from modified tracking
        let mut modified = self.modified.write().expect("modified lock");
        modified.remove(virtual_path);

        tracing::trace!(path = %virtual_path.display(), "Marked path as deleted");
        Ok(())
    }

    /// Unmark a path as deleted (when recreating a previously deleted file).
    pub fn unmark_deleted(&self, virtual_path: &Path) {
        let mut whiteouts = self.whiteouts.write().expect("whiteouts lock");
        whiteouts.remove(virtual_path);
    }

    /// Copy a file from a source path to scratch.
    ///
    /// This is the "copy-up" operation that happens on first write
    /// to a file from the base layer.
    pub fn copy_to_scratch(
        &self,
        virtual_path: &Path,
        source: &Path,
    ) -> Result<PathBuf, ScratchError> {
        let scratch_path = self.scratch_path(virtual_path);

        // Create parent directories
        if let Some(parent) = scratch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Copy the file
        std::fs::copy(source, &scratch_path)?;

        // Track as modified
        let mut modified = self.modified.write().expect("modified lock");
        modified.insert(virtual_path.to_path_buf());

        // Unmark as deleted if it was
        self.unmark_deleted(virtual_path);

        tracing::trace!(
            virtual_path = %virtual_path.display(),
            scratch_path = %scratch_path.display(),
            "Copied file to scratch"
        );

        Ok(scratch_path)
    }

    /// Create a new empty file in scratch.
    ///
    /// Returns a File handle for writing.
    pub fn create_file(&self, virtual_path: &Path) -> Result<File, ScratchError> {
        let scratch_path = self.scratch_path(virtual_path);

        // Create parent directories
        if let Some(parent) = scratch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::create(&scratch_path)?;

        // Track as modified
        let mut modified = self.modified.write().expect("modified lock");
        modified.insert(virtual_path.to_path_buf());

        // Unmark as deleted if it was
        self.unmark_deleted(virtual_path);

        tracing::trace!(path = %virtual_path.display(), "Created file in scratch");

        Ok(file)
    }

    /// Open an existing file in scratch for read/write.
    pub fn open_file(&self, virtual_path: &Path) -> Result<File, ScratchError> {
        let scratch_path = self.scratch_path(virtual_path);

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&scratch_path)?;

        Ok(file)
    }

    /// Create a directory in scratch.
    pub fn create_dir(&self, virtual_path: &Path) -> Result<(), ScratchError> {
        let scratch_path = self.scratch_path(virtual_path);
        std::fs::create_dir_all(&scratch_path)?;

        // Track as modified
        let mut modified = self.modified.write().expect("modified lock");
        modified.insert(virtual_path.to_path_buf());

        // Unmark as deleted if it was
        self.unmark_deleted(virtual_path);

        tracing::trace!(path = %virtual_path.display(), "Created directory in scratch");

        Ok(())
    }

    /// Remove a directory from scratch.
    pub fn remove_dir(&self, virtual_path: &Path) -> Result<(), ScratchError> {
        self.mark_deleted(virtual_path)
    }

    /// Rename a path in scratch.
    ///
    /// Both old and new paths should be virtual paths.
    pub fn rename(&self, old_virtual: &Path, new_virtual: &Path) -> Result<(), ScratchError> {
        let old_scratch = self.scratch_path(old_virtual);
        let new_scratch = self.scratch_path(new_virtual);

        // Create parent directories for destination
        if let Some(parent) = new_scratch.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Perform the rename
        std::fs::rename(&old_scratch, &new_scratch)?;

        // Update tracking
        {
            let mut modified = self.modified.write().expect("modified lock");
            modified.remove(old_virtual);
            modified.insert(new_virtual.to_path_buf());
        }

        // Mark old path as deleted (whiteout) so base layer file is hidden
        {
            let mut whiteouts = self.whiteouts.write().expect("whiteouts lock");
            whiteouts.insert(old_virtual.to_path_buf());
            whiteouts.remove(new_virtual);
        }

        tracing::trace!(
            old = %old_virtual.display(),
            new = %new_virtual.display(),
            "Renamed path in scratch"
        );

        Ok(())
    }

    /// Get all virtual paths that have been modified.
    ///
    /// Useful for commit operations.
    pub fn modified_paths(&self) -> Vec<PathBuf> {
        let modified = self.modified.read().expect("modified lock");
        modified.iter().cloned().collect()
    }

    /// Get all virtual paths that have been deleted (whiteouts).
    ///
    /// Useful for commit operations.
    pub fn deleted_paths(&self) -> Vec<PathBuf> {
        let whiteouts = self.whiteouts.read().expect("whiteouts lock");
        whiteouts.iter().cloned().collect()
    }

    /// Check if there are any changes (modified or deleted files).
    pub fn has_changes(&self) -> bool {
        let modified = self.modified.read().expect("modified lock");
        let whiteouts = self.whiteouts.read().expect("whiteouts lock");
        !modified.is_empty() || !whiteouts.is_empty()
    }

    /// Clean up the scratch directory.
    ///
    /// Removes all files and the directory itself.
    pub fn cleanup(&self) -> Result<(), ScratchError> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
            tracing::debug!(path = %self.root.display(), "Cleaned up scratch directory");
        }
        Ok(())
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            tracing::warn!(
                err = ?e,
                path = %self.root.display(),
                "Failed to clean up scratch directory"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    fn test_scratch() -> (TempDir, ScratchDir) {
        let temp = TempDir::new().unwrap();
        let scratch = ScratchDir::at_path(temp.path().to_path_buf()).unwrap();
        (temp, scratch)
    }

    #[test]
    fn test_scratch_path_conversion() {
        let (_temp, scratch) = test_scratch();

        let virtual_path = Path::new("/bin/foo");
        let scratch_path = scratch.scratch_path(virtual_path);

        assert!(scratch_path.ends_with("bin/foo"));
        assert!(scratch_path.starts_with(scratch.root()));
    }

    #[test]
    fn test_create_file() {
        let (_temp, scratch) = test_scratch();

        let virtual_path = Path::new("/test/file.txt");
        let mut file = scratch.create_file(virtual_path).unwrap();

        // Write some content
        file.write_all(b"hello").unwrap();
        drop(file);

        // Verify file exists
        assert!(scratch.is_in_scratch(virtual_path));
        assert!(scratch.exists_in_scratch(virtual_path));

        // Verify content
        let scratch_path = scratch.scratch_path(virtual_path);
        let content = std::fs::read_to_string(&scratch_path).unwrap();
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_create_directory() {
        let (_temp, scratch) = test_scratch();

        let virtual_path = Path::new("/test/subdir");
        scratch.create_dir(virtual_path).unwrap();

        assert!(scratch.is_in_scratch(virtual_path));
        assert!(scratch.scratch_path(virtual_path).is_dir());
    }

    #[test]
    fn test_whiteout() {
        let (_temp, scratch) = test_scratch();

        // Create a file first
        let virtual_path = Path::new("/test/deleteme.txt");
        scratch.create_file(virtual_path).unwrap();
        assert!(!scratch.is_deleted(virtual_path));
        assert!(scratch.is_in_scratch(virtual_path));

        // Mark as deleted
        scratch.mark_deleted(virtual_path).unwrap();

        // Should now be deleted
        assert!(scratch.is_deleted(virtual_path));
        assert!(!scratch.is_in_scratch(virtual_path));
        assert!(!scratch.exists_in_scratch(virtual_path));
    }

    #[test]
    fn test_recreate_after_delete() {
        let (_temp, scratch) = test_scratch();

        let virtual_path = Path::new("/test/file.txt");

        // Mark as deleted (simulating deletion of base layer file)
        scratch.mark_deleted(virtual_path).unwrap();
        assert!(scratch.is_deleted(virtual_path));

        // Recreate the file
        scratch.create_file(virtual_path).unwrap();

        // Should no longer be deleted
        assert!(!scratch.is_deleted(virtual_path));
        assert!(scratch.is_in_scratch(virtual_path));
    }

    #[test]
    fn test_copy_to_scratch() {
        let (temp, scratch) = test_scratch();

        // Create a source file
        let source_path = temp.path().join("source.txt");
        std::fs::write(&source_path, "source content").unwrap();

        // Copy to scratch
        let virtual_path = Path::new("/copied.txt");
        scratch.copy_to_scratch(virtual_path, &source_path).unwrap();

        // Verify copy
        assert!(scratch.is_in_scratch(virtual_path));
        let content = std::fs::read_to_string(scratch.scratch_path(virtual_path)).unwrap();
        assert_eq!(content, "source content");
    }

    #[test]
    fn test_rename() {
        let (_temp, scratch) = test_scratch();

        // Create source file
        let old_path = Path::new("/old.txt");
        let mut file = scratch.create_file(old_path).unwrap();
        file.write_all(b"content").unwrap();
        drop(file);

        // Rename
        let new_path = Path::new("/new.txt");
        scratch.rename(old_path, new_path).unwrap();

        // Old path should be whiteout, new path should exist
        assert!(scratch.is_deleted(old_path));
        assert!(!scratch.exists_in_scratch(old_path));
        assert!(scratch.is_in_scratch(new_path));
        assert!(scratch.exists_in_scratch(new_path));
    }

    #[test]
    fn test_modified_and_deleted_paths() {
        let (_temp, scratch) = test_scratch();

        // Create some files
        scratch.create_file(Path::new("/a.txt")).unwrap();
        scratch.create_file(Path::new("/b.txt")).unwrap();

        // Delete one
        scratch.mark_deleted(Path::new("/c.txt")).unwrap();

        let modified = scratch.modified_paths();
        let deleted = scratch.deleted_paths();

        assert_eq!(modified.len(), 2);
        assert_eq!(deleted.len(), 1);
        assert!(modified.contains(&PathBuf::from("/a.txt")));
        assert!(modified.contains(&PathBuf::from("/b.txt")));
        assert!(deleted.contains(&PathBuf::from("/c.txt")));
    }

    #[test]
    fn test_has_changes() {
        let (_temp, scratch) = test_scratch();

        assert!(!scratch.has_changes());

        scratch.create_file(Path::new("/file.txt")).unwrap();
        assert!(scratch.has_changes());
    }
}
