# macOS SPFS: FuseWithScratch Implementation Plan

## Executive Summary

This plan details the implementation of write support for SPFS on macOS by introducing a new `FuseWithScratch` mount backend. This backend provides copy-on-write semantics using a userspace scratch directory, enabling editable runtimes without requiring overlayfs (which doesn't exist on macOS).

**Context**: Phase 1 of macOS support (read-only) is complete. This plan covers Phase 2 (write support) and remaining Phase 1 items.

**Key Decision**: We are adding a new `MountBackend::FuseWithScratch` variant rather than overloading `FuseOnly` because:
1. `FuseOnly` explicitly means "read-only FUSE without write support"
2. The new variant has fundamentally different capabilities and implementation
3. It could potentially be used on Linux as a fallback when overlayfs isn't available

---

## Current State Analysis

### What's Complete (Phase 1)

| Task | Status | Notes |
|------|--------|-------|
| 1.1 Project Structure | DONE | `crates/spfs-vfs/src/macos/` exists |
| 1.2 Process Ancestry | DONE | `process.rs` with libproc |
| 1.3 Handle Types | DONE | `handle.rs` |
| 1.4 Mount Implementation | DONE | `mount.rs` (read-only) |
| 1.5 Router Implementation | DONE | `router.rs` |
| 1.6 Module Exports | DONE | `mod.rs` |
| 1.7 CLI Binary | DONE | `spfs-fuse-macos` builds |
| 1.8 MountBackend Enum | PARTIAL | Uses `FuseOnly`, needs `FuseWithScratch` |
| 1.9 RuntimeConfigurator | DONE | `env_macos.rs` |
| 1.10 Status Module | DONE | `status_macos.rs` |
| 1.11 Platform Switching | DONE | `lib.rs` cfg attributes |
| 1.12 Integration Testing | NOT STARTED | Blocked on manual testing |

### What's Missing for Write Support

1. **New `FuseWithScratch` variant** in `MountBackend` enum
2. **Scratch directory management** - per-mount temp directory for COW
3. **FUSE write operations** - `write()`, `create()`, `mkdir()`, `unlink()`, etc.
4. **Copy-up semantics** - copy file from repo to scratch on first write
5. **Whiteout tracking** - track deleted files
6. **Commit integration** - read changes from scratch for `spfs commit`

### How Linux Does It (for reference)

```
┌─────────────────────────────────────┐
│           OverlayFS                 │
│  ┌─────────────────────────────┐    │
│  │ upper: tmpfs (writes here)  │    │  ← Kernel handles COW
│  ├─────────────────────────────┤    │
│  │ lower: FUSE or rendered     │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘
```

Linux uses overlayfs which handles copy-on-write at the kernel level. The `upper_dir` captures all modifications.

### How macOS Will Do It

```
┌─────────────────────────────────────┐
│    macFUSE + Userspace COW          │
│  ┌─────────────────────────────┐    │
│  │ scratch: ~/spfs-scratch/    │    │  ← We handle COW in userspace
│  │   (modified/new files)      │    │
│  ├─────────────────────────────┤    │
│  │ whiteouts: HashSet<path>    │    │  ← Track deletions in memory
│  ├─────────────────────────────┤    │
│  │ base: SPFS repos            │    │
│  │   (read from repos)         │    │
│  └─────────────────────────────┘    │
└─────────────────────────────────────┘
```

We implement copy-on-write in userspace within the FUSE filesystem.

---

## Desired End State

After completing this plan:

1. **Write Support on macOS**: `spfs shell --edit` works on macOS
2. **Commit Changes**: `spfs commit layer` captures changes from scratch directory
3. **Parity with Linux**: Editable runtimes work the same way from user perspective
4. **Clean Architecture**: New `FuseWithScratch` backend is clearly documented

### Verification Criteria

```bash
# Editable runtime works
spfs shell --edit my/package
echo "new content" > /spfs/newfile.txt
cat /spfs/newfile.txt  # Shows "new content"

# Commit captures changes
spfs commit layer -m "Added newfile"

# Deletions work
rm /spfs/some-existing-file
ls /spfs/  # File no longer visible

# Non-editable is still read-only
spfs run my/package -- touch /spfs/test
# Should fail with "Read-only file system"
```

---

## What We're NOT Doing

1. **Durable runtimes on macOS** - Focus on transient editable runtimes first
2. **Live layer support** - Phase 3 concern
3. **Performance optimization** - Get it working first, optimize later
4. **Hard links in scratch** - Use simple file copies initially

---

## Architecture Overview

### Mount Backend Selection

```rust
pub enum MountBackend {
    OverlayFsWithRenders,  // Linux: pre-render + overlayfs
    OverlayFsWithFuse,     // Linux: FUSE lower + overlayfs
    FuseOnly,              // Any: read-only FUSE (current macOS default)
    FuseWithScratch,       // NEW: FUSE + userspace COW (macOS editable)
    WinFsp,                // Windows: read-only WinFSP
}
```

### Scratch Directory Structure

```
/tmp/spfs-runtime-{runtime_name}/
├── scratch/           # Modified/new files (COW copies)
│   ├── bin/
│   │   └── modified-binary
│   └── new-file.txt
├── whiteouts.json     # Deleted file paths (or in-memory)
└── metadata.json      # Runtime info
```

### Data Flow for Writes

```
1. Application writes to /spfs/foo/bar
         │
         ▼
2. macFUSE → Router → Mount::write()
         │
         ▼
3. Mount checks: is file in scratch?
   ├── YES: write directly to scratch/foo/bar
   └── NO: copy-up from repo to scratch/foo/bar, then write
         │
         ▼
4. Return success to application
```

### Data Flow for Reads (with scratch)

```
1. Application reads /spfs/foo/bar
         │
         ▼
2. macFUSE → Router → Mount::read()
         │
         ▼
3. Mount checks: is file deleted (whiteout)?
   └── YES: return ENOENT
         │
         ▼
4. Mount checks: is file in scratch?
   ├── YES: read from scratch/foo/bar
   └── NO: read from repo (original behavior)
```

---

## Implementation Phases

### Phase 1 Completion: Remaining Items

Before starting write support, complete these Phase 1 items:

#### Task 1.12: Integration Testing (2-3 days)

**Files**: `crates/spfs-vfs/src/macos/tests.rs`

Create integration tests for read-only functionality:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_ancestry() {
        let ancestry = get_parent_pids_macos(None).unwrap();
        assert!(!ancestry.is_empty());
        assert_eq!(ancestry[0], std::process::id() as i32);
    }

    #[test]
    fn test_router_empty_default() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let router = Router::new(vec![]).await.unwrap();
            assert_eq!(router.mount_count(), 0);
        });
    }

    #[test]
    fn test_mount_empty() {
        let mount = Mount::empty().unwrap();
        // Verify root inode exists
    }
}
```

**Manual Testing Checklist**:
- [ ] `spfs-fuse-macos service` starts on macOS with macFUSE
- [ ] `spfs-fuse-macos mount <ref>` registers with service
- [ ] `ls /spfs` shows correct files
- [ ] Two concurrent runtimes see isolated views
- [ ] Service shutdown is clean

---

### Phase 2: FuseWithScratch Implementation

#### Task 2.1: Add MountBackend::FuseWithScratch Variant (0.5 days)

**File**: `crates/spfs/src/runtime/storage.rs`

```rust
/// Identifies a filesystem backend for spfs
#[derive(
    Default, Clone, Copy, Debug, Eq, PartialEq,
    strum::Display, strum::EnumString, strum::VariantNames,
    Serialize, Deserialize,
)]
pub enum MountBackend {
    /// Renders each layer to a folder on disk, before mounting
    /// the whole stack as lower directories in overlayfs. Edits
    /// are stored in the overlayfs upper directory.
    #[cfg_attr(target_os = "linux", default)]
    OverlayFsWithRenders,
    
    /// Mounts a fuse filesystem as the lower directory to
    /// overlayfs, using the overlayfs upper directory for edits
    OverlayFsWithFuse,
    
    /// Mounts a fuse filesystem directly (read-only)
    FuseOnly,
    
    /// Mounts a fuse filesystem with a userspace scratch directory
    /// for copy-on-write semantics. Used for editable runtimes on
    /// platforms without overlayfs (macOS).
    #[cfg_attr(target_os = "macos", default)]
    FuseWithScratch,
    
    /// Leverages the win file system protocol system to present
    /// dynamic file system entries to runtime processes
    #[cfg_attr(windows, default)]
    WinFsp,
}

impl MountBackend {
    // ... existing methods ...

    pub fn is_fuse_with_scratch(&self) -> bool {
        matches!(self, Self::FuseWithScratch)
    }

    pub fn is_fuse(&self) -> bool {
        match self {
            MountBackend::OverlayFsWithRenders => false,
            MountBackend::OverlayFsWithFuse => true,
            MountBackend::FuseOnly => true,
            MountBackend::FuseWithScratch => true,
            MountBackend::WinFsp => false,
        }
    }

    pub fn requires_localization(&self) -> bool {
        match self {
            Self::OverlayFsWithRenders => true,
            Self::OverlayFsWithFuse => false,
            Self::FuseOnly => false,
            Self::FuseWithScratch => false,
            Self::WinFsp => false,
        }
    }

    /// Whether this backend supports editable runtimes
    pub fn supports_editable(&self) -> bool {
        match self {
            Self::OverlayFsWithRenders => true,
            Self::OverlayFsWithFuse => true,
            Self::FuseOnly => false,
            Self::FuseWithScratch => true,
            Self::WinFsp => false,
        }
    }
}
```

**Acceptance Criteria**:
- [ ] `cargo check -p spfs` passes on all platforms
- [ ] `FuseWithScratch` is default on macOS
- [ ] Existing tests pass

---

#### Task 2.2: Scratch Directory Management (2-3 days)

**File**: `crates/spfs-vfs/src/macos/scratch.rs` (new)

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! Scratch directory management for copy-on-write semantics
//!
//! Manages a temporary directory that stores modified files,
//! new files, and tracks deletions (whiteouts).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use spfs::encoding::Digest;

/// Manages the scratch directory for an editable mount
#[derive(Debug)]
pub struct ScratchDir {
    /// Root path of the scratch directory
    root: PathBuf,
    /// Set of paths that have been deleted (whiteouts)
    whiteouts: RwLock<HashSet<PathBuf>>,
    /// Set of paths that exist in scratch (for quick lookup)
    modified: RwLock<HashSet<PathBuf>>,
}

impl ScratchDir {
    /// Create a new scratch directory
    pub fn new(runtime_name: &str) -> std::io::Result<Self> {
        let root = std::env::temp_dir()
            .join(format!("spfs-scratch-{}", runtime_name));
        std::fs::create_dir_all(&root)?;
        
        Ok(Self {
            root,
            whiteouts: RwLock::new(HashSet::new()),
            modified: RwLock::new(HashSet::new()),
        })
    }

    /// Get the scratch directory root path
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the path in scratch for a given virtual path
    pub fn scratch_path(&self, virtual_path: &Path) -> PathBuf {
        self.root.join(virtual_path.strip_prefix("/").unwrap_or(virtual_path))
    }

    /// Check if a path is deleted (whiteout)
    pub fn is_deleted(&self, path: &Path) -> bool {
        let whiteouts = self.whiteouts.read().expect("lock");
        whiteouts.contains(path)
    }

    /// Check if a path exists in scratch
    pub fn is_in_scratch(&self, path: &Path) -> bool {
        let modified = self.modified.read().expect("lock");
        modified.contains(path)
    }

    /// Mark a path as deleted
    pub fn mark_deleted(&self, path: &Path) {
        let mut whiteouts = self.whiteouts.write().expect("lock");
        whiteouts.insert(path.to_path_buf());
        
        // Also remove from modified if present
        let mut modified = self.modified.write().expect("lock");
        modified.remove(path);
    }

    /// Unmark a path as deleted (file recreated)
    pub fn unmark_deleted(&self, path: &Path) {
        let mut whiteouts = self.whiteouts.write().expect("lock");
        whiteouts.remove(path);
    }

    /// Copy a file from source to scratch, creating parent dirs
    pub fn copy_to_scratch(&self, virtual_path: &Path, source: &Path) -> std::io::Result<PathBuf> {
        let scratch_path = self.scratch_path(virtual_path);
        
        if let Some(parent) = scratch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::copy(source, &scratch_path)?;
        
        let mut modified = self.modified.write().expect("lock");
        modified.insert(virtual_path.to_path_buf());
        
        Ok(scratch_path)
    }

    /// Create a new file in scratch
    pub fn create_in_scratch(&self, virtual_path: &Path) -> std::io::Result<std::fs::File> {
        let scratch_path = self.scratch_path(virtual_path);
        
        if let Some(parent) = scratch_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let file = std::fs::File::create(&scratch_path)?;
        
        // Unmark as deleted if it was
        self.unmark_deleted(virtual_path);
        
        let mut modified = self.modified.write().expect("lock");
        modified.insert(virtual_path.to_path_buf());
        
        Ok(file)
    }

    /// Create a directory in scratch
    pub fn create_dir_in_scratch(&self, virtual_path: &Path) -> std::io::Result<()> {
        let scratch_path = self.scratch_path(virtual_path);
        std::fs::create_dir_all(&scratch_path)?;
        
        self.unmark_deleted(virtual_path);
        
        let mut modified = self.modified.write().expect("lock");
        modified.insert(virtual_path.to_path_buf());
        
        Ok(())
    }

    /// Get all modified paths (for commit)
    pub fn modified_paths(&self) -> Vec<PathBuf> {
        let modified = self.modified.read().expect("lock");
        modified.iter().cloned().collect()
    }

    /// Get all deleted paths (for commit)
    pub fn deleted_paths(&self) -> Vec<PathBuf> {
        let whiteouts = self.whiteouts.read().expect("lock");
        whiteouts.iter().cloned().collect()
    }

    /// Clean up the scratch directory
    pub fn cleanup(&self) -> std::io::Result<()> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            tracing::warn!(err = ?e, "Failed to clean up scratch directory");
        }
    }
}
```

**Acceptance Criteria**:
- [ ] Unit tests for ScratchDir pass
- [ ] Files are correctly copied to scratch
- [ ] Whiteouts are tracked
- [ ] Cleanup works on drop

---

#### Task 2.3: Update Mount for Write Operations (4-5 days)

**File**: `crates/spfs-vfs/src/macos/mount.rs`

Add write support to the Mount struct:

```rust
// Add to Mount struct
pub struct Mount {
    rt: tokio::runtime::Handle,
    repos: Vec<Arc<RepositoryHandle>>,
    ttl: Duration,
    next_inode: AtomicU64,
    next_handle: AtomicU64,
    inodes: DashMap<u64, Arc<Entry<u64>>>,
    handles: DashMap<u64, Handle>,
    fs_creation_time: SystemTime,
    uid: u32,
    gid: u32,
    
    // NEW: scratch directory for editable mounts
    scratch: Option<ScratchDir>,
    // NEW: map virtual path to inode for scratch files
    scratch_inodes: DashMap<PathBuf, u64>,
}

impl Mount {
    /// Create a new editable mount with scratch directory
    pub fn new_editable(
        rt: tokio::runtime::Handle,
        repos: Vec<Arc<RepositoryHandle>>,
        manifest: Manifest,
        runtime_name: &str,
    ) -> spfs::Result<Self> {
        let scratch = ScratchDir::new(runtime_name)
            .map_err(|e| spfs::Error::String(format!("Failed to create scratch: {e}")))?;
        
        let mut mount = Self::new(rt, repos, manifest)?;
        mount.scratch = Some(scratch);
        Ok(mount)
    }

    /// Check if this mount is editable
    pub fn is_editable(&self) -> bool {
        self.scratch.is_some()
    }

    // ... existing methods ...

    /// Write data to a file
    pub fn write(
        &self,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        _flags: i32,
        reply: fuser::ReplyWrite,
    ) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let Some(handle) = self.handles.get(&fh) else {
            reply.error(libc::EBADF);
            return;
        };

        match handle.value() {
            Handle::ScratchFile { file, .. } => {
                use std::os::unix::prelude::FileExt;
                match file.write_at(data, offset as u64) {
                    Ok(written) => reply.written(written as u32),
                    Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
                }
            }
            Handle::BlobFile { entry, file } => {
                // Copy-up: copy to scratch first
                let virtual_path = self.get_path_for_inode(ino);
                // ... implement copy-up logic
            }
            _ => reply.error(libc::EISDIR),
        }
    }

    /// Create a new file
    pub fn create(
        &self,
        parent: u64,
        name: &OsStr,
        mode: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        // Get parent path
        let parent_path = self.get_path_for_inode(parent);
        let file_path = parent_path.join(name);

        // Create in scratch
        match scratch.create_in_scratch(&file_path) {
            Ok(file) => {
                let ino = self.allocate_inode();
                // Create entry, store handle
                // ... 
                reply.created(&self.ttl, &attr, 0, fh, 0);
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    /// Delete a file
    pub fn unlink(&self, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let parent_path = self.get_path_for_inode(parent);
        let file_path = parent_path.join(name);

        // Mark as deleted (whiteout)
        scratch.mark_deleted(&file_path);
        
        // Remove from inode table if in scratch
        // ...

        reply.ok();
    }

    /// Create a directory
    pub fn mkdir(
        &self,
        parent: u64,
        name: &OsStr,
        mode: u32,
        reply: fuser::ReplyEntry,
    ) {
        let Some(scratch) = &self.scratch else {
            reply.error(libc::EROFS);
            return;
        };

        let parent_path = self.get_path_for_inode(parent);
        let dir_path = parent_path.join(name);

        match scratch.create_dir_in_scratch(&dir_path) {
            Ok(()) => {
                let ino = self.allocate_inode();
                // ... create entry and return
                reply.entry(&self.ttl, &attr, 0);
            }
            Err(e) => reply.error(e.raw_os_error().unwrap_or(libc::EIO)),
        }
    }

    // ... implement rmdir, rename, setattr, truncate similarly
}
```

Add new handle type for scratch files:

**File**: `crates/spfs-vfs/src/macos/handle.rs`

```rust
pub enum Handle {
    BlobFile { ... },
    BlobStream { ... },
    Tree { ... },
    
    /// A file in the scratch directory (read-write)
    ScratchFile {
        entry: Arc<Entry<u64>>,
        file: std::fs::File,
        virtual_path: PathBuf,
    },
}
```

**Acceptance Criteria**:
- [ ] `write()` works for scratch files
- [ ] `create()` creates new files in scratch
- [ ] `unlink()` marks files as deleted
- [ ] `mkdir()` creates directories in scratch
- [ ] Copy-up works on first write to repo file

---

#### Task 2.4: Update Router for Write Operations (1 day)

**File**: `crates/spfs-vfs/src/macos/router.rs`

Add write operation delegation:

```rust
impl Filesystem for Router {
    // ... existing methods ...

    fn write(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        data: &[u8],
        write_flags: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: fuser::ReplyWrite,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.write(ino, fh, offset, data, flags, reply);
    }

    fn create(
        &mut self,
        req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: fuser::ReplyCreate,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.create(parent, name, mode, flags, reply);
    }

    fn unlink(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.unlink(parent, name, reply);
    }

    fn mkdir(
        &mut self,
        req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: fuser::ReplyEntry,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.mkdir(parent, name, mode, reply);
    }

    fn rmdir(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: fuser::ReplyEmpty) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.rmdir(parent, name, reply);
    }

    fn rename(
        &mut self,
        req: &Request<'_>,
        parent: u64,
        name: &OsStr,
        newparent: u64,
        newname: &OsStr,
        flags: u32,
        reply: fuser::ReplyEmpty,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.rename(parent, name, newparent, newname, flags, reply);
    }

    fn setattr(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<fuser::TimeOrNow>,
        mtime: Option<fuser::TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<u64>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.setattr(ino, mode, uid, gid, size, atime, mtime, fh, reply);
    }
}
```

---

#### Task 2.5: Update gRPC Mount Request (1 day)

**File**: `crates/spfs-vfs/src/proto/defs/vfs.proto`

```protobuf
message MountRequest {
    uint32 root_pid = 1;
    string env_spec = 2;
    bool editable = 3;  // NEW: whether to create editable mount
    string runtime_name = 4;  // NEW: for scratch directory naming
}
```

**File**: `crates/spfs-vfs/src/macos/service.rs`

Update the mount handler:

```rust
async fn mount(
    &self,
    request: Request<MountRequest>,
) -> Result<Response<MountResponse>, Status> {
    let req = request.into_inner();

    let env_spec: EnvSpec = req.env_spec.parse()
        .map_err(|e| Status::invalid_argument(format!("Invalid env spec: {e}")))?;

    let router_guard = self.router.lock().await;
    let router = router_guard.as_ref().ok_or_else(|| {
        Status::failed_precondition("Service not running")
    })?;

    // Pass editable flag to mount
    router.mount(req.root_pid, env_spec, req.editable, &req.runtime_name).await
        .map_err(|e| Status::internal(format!("Failed to mount: {e}")))?;

    Ok(Response::new(MountResponse {}))
}
```

---

#### Task 2.6: Update status_macos.rs for Editable Runtimes (2 days)

**File**: `crates/spfs/src/status_macos.rs`

```rust
/// Initialize the current runtime
pub async fn initialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    tracing::debug!("computing runtime manifest");
    let _manifest = super::compute_runtime_manifest(rt).await?;

    match rt.config.mount_backend {
        runtime::MountBackend::FuseOnly => {
            if rt.status.editable {
                return Err(Error::String(
                    "FuseOnly backend does not support editable runtimes. \
                     Use FuseWithScratch instead.".to_string()
                ));
            }
            mount_env_fuse_readonly(rt).await?;
        }
        runtime::MountBackend::FuseWithScratch => {
            mount_env_fuse_with_scratch(rt).await?;
        }
        _ => {
            return Err(Error::String(format!(
                "Backend {} is not supported on macOS",
                rt.config.mount_backend
            )));
        }
    }
    
    Ok(RenderSummary::default())
}

async fn mount_env_fuse_readonly(rt: &runtime::Runtime) -> Result<()> {
    let env_spec = rt.status.stack.iter_bottom_up().collect::<EnvSpec>();
    let exe = crate::which_spfs("fuse-macos")
        .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;
    
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("mount")
        .arg("--root-process").arg(rt.status.owner.unwrap().to_string())
        .arg(env_spec.to_string());
    
    run_mount_command(cmd).await
}

async fn mount_env_fuse_with_scratch(rt: &runtime::Runtime) -> Result<()> {
    let env_spec = rt.status.stack.iter_bottom_up().collect::<EnvSpec>();
    let exe = crate::which_spfs("fuse-macos")
        .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;
    
    let mut cmd = tokio::process::Command::new(exe);
    cmd.arg("mount")
        .arg("--root-process").arg(rt.status.owner.unwrap().to_string())
        .arg("--editable")  // NEW flag
        .arg("--runtime-name").arg(rt.name())
        .arg(env_spec.to_string());
    
    run_mount_command(cmd).await
}

/// Check if runtime has uncommitted changes
pub fn is_runtime_dirty(rt: &runtime::Runtime) -> bool {
    match rt.config.mount_backend {
        runtime::MountBackend::FuseWithScratch => {
            // Check if scratch directory has changes
            let scratch_root = std::env::temp_dir()
                .join(format!("spfs-scratch-{}", rt.name()));
            
            if !scratch_root.exists() {
                return false;
            }
            
            // Check if there are any files in scratch
            std::fs::read_dir(&scratch_root)
                .map(|mut entries| entries.next().is_some())
                .unwrap_or(false)
        }
        _ => false,
    }
}
```

---

#### Task 2.7: Update Commit for FuseWithScratch (2 days)

**File**: `crates/spfs/src/commit.rs`

Update commit to read from scratch directory on macOS:

```rust
/// Commit the working file changes of a runtime to a new layer.
pub async fn commit_layer(&self, runtime: &mut runtime::Runtime) -> Result<graph::Layer> {
    let changes_dir = match runtime.config.mount_backend {
        MountBackend::OverlayFsWithRenders | MountBackend::OverlayFsWithFuse => {
            // Linux: read from upper_dir
            runtime.config.upper_dir.clone()
        }
        MountBackend::FuseWithScratch => {
            // macOS: read from scratch directory
            std::env::temp_dir().join(format!("spfs-scratch-{}", runtime.name()))
        }
        _ => {
            return Err(Error::String(format!(
                "Backend {} does not support commit",
                runtime.config.mount_backend
            )));
        }
    };
    
    let manifest = self.commit_dir(&changes_dir).await?;
    self.commit_manifest(manifest, runtime).await
}
```

---

#### Task 2.8: CLI Updates for --editable (1 day)

**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

Add editable flag to mount command:

```rust
#[derive(Debug, Args)]
struct CmdMount {
    #[clap(long)]
    root_process: Option<u32>,

    #[clap(long, default_value = "127.0.0.1:37738")]
    service: SocketAddr,

    /// Create an editable mount with scratch directory
    #[clap(long)]
    editable: bool,

    /// Runtime name (required for editable mounts)
    #[clap(long)]
    runtime_name: Option<String>,

    #[clap(name = "REF")]
    reference: EnvSpec,
}

impl CmdMount {
    async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        // ... existing code ...

        let runtime_name = self.runtime_name.clone().unwrap_or_else(|| {
            format!("runtime-{}", std::process::id())
        });

        client
            .mount(Request::new(proto::MountRequest {
                root_pid,
                env_spec: self.reference.to_string(),
                editable: self.editable,
                runtime_name,
            }))
            .await
            .into_diagnostic()?;

        Ok(0)
    }
}
```

---

### Phase 3: Polish and Integration

#### Task 3.1: Integration Testing for Write Support (3 days)

Create comprehensive tests:

```rust
#[cfg(test)]
mod write_tests {
    #[test]
    fn test_create_file_in_scratch() {
        // Create editable mount
        // Write new file
        // Verify file exists
    }

    #[test]
    fn test_modify_existing_file() {
        // Create editable mount with existing content
        // Modify file (triggers copy-up)
        // Verify modification
    }

    #[test]
    fn test_delete_file_whiteout() {
        // Create editable mount
        // Delete existing file
        // Verify file no longer visible
    }

    #[test]
    fn test_commit_changes() {
        // Create editable mount
        // Make changes
        // Commit
        // Verify new layer created
    }
}
```

---

#### Task 3.2: Documentation Update (1 day)

Update `docs/spfs/develop/macos-fuse-architecture.md`:

- Add section on `FuseWithScratch` backend
- Document scratch directory structure
- Add troubleshooting for write issues

---

## Success Criteria

### Automated Verification
- [ ] `cargo check -p spfs` passes with new `FuseWithScratch` variant
- [ ] `cargo build -p spfs-cli-fuse-macos` succeeds
- [ ] Unit tests pass for scratch directory management
- [ ] Unit tests pass for write operations

### Manual Verification
- [ ] `spfs shell --edit my/package` creates editable runtime on macOS
- [ ] File writes succeed in editable runtime
- [ ] File deletions work (whiteouts)
- [ ] `spfs commit layer` captures changes from scratch
- [ ] Non-editable runtime rejects writes with EROFS
- [ ] Multiple editable runtimes work concurrently

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| Copy-up performance on large files | Medium | Medium | Implement lazy copy-up, only copy written regions |
| Whiteout persistence across remount | Medium | Low | Store whiteouts in file, reload on mount |
| Scratch directory cleanup on crash | Medium | Low | Use temp directory naming, add cleanup on service start |
| Race conditions in scratch access | Low | High | Use proper locking in ScratchDir |

---

## Dependencies

```
Task 1.12 (Tests) ──────► Phase 2 Start
                              │
Task 2.1 (MountBackend) ──────┼──► Task 2.2 (ScratchDir)
                              │         │
                              │         ▼
                              │    Task 2.3 (Mount writes)
                              │         │
                              │         ▼
                              │    Task 2.4 (Router writes)
                              │         │
Task 2.5 (Proto) ─────────────┼─────────┤
                              │         │
                              │         ▼
                              │    Task 2.6 (status_macos)
                              │         │
                              │         ▼
                              │    Task 2.7 (Commit)
                              │         │
                              │         ▼
                              │    Task 2.8 (CLI)
                              │         │
                              ▼         ▼
                         Phase 3 (Polish)
```

---

## References

- Original macOS Plan: `.llm/shared/plans/2025-11-29-spfs-macos-implementation.md`
- Architecture Doc: `docs/spfs/develop/macos-fuse-architecture.md`
- FUSE Context: `.llm/shared/context/2025-11-28-spk-spfs-fuse.md`
- Linux commit flow: `crates/spfs/src/commit.rs`
- Overlayfs mount: `crates/spfs/src/env.rs`
