# macOS SPFS Implementation Plan

## Executive Summary

This plan details the implementation of SPFS on macOS using macFUSE with WinFSP-style PID-based process isolation. The approach adapts the proven Windows WinFSP router pattern to macOS, using the `fuser` crate with `macfuse-4-compat` feature and the `libproc` crate for process ancestry tracking.

**Total Estimated Effort**: 10-16 weeks across 3 phases
- Phase 1: Read-Only MVP (4-6 weeks)
- Phase 2: Write Support (4-6 weeks)  
- Phase 3: Polish and Production (2-4 weeks)

**Key Technical Decisions**:
- FUSE Backend: macFUSE + `fuser` crate (feature: `macfuse-4-compat`)
- Process Isolation: Singleton Router pattern (ported from `winfsp/router.rs`)
- Process Ancestry: `libproc` crate with `proc_pidinfo()` API
- Control Plane: Reuse existing gRPC proto (`vfs.proto`) with tonic

---

## Timeline Overview

```
Week:  1   2   3   4   5   6   7   8   9  10  11  12  13  14  15  16
       ├───────────────────────┤
       │     Phase 1: MVP      │
       │   (Read-Only Mount)   │
                               ├───────────────────────┤
                               │    Phase 2: Write     │
                               │      Support          │
                                                       ├───────────────┤
                                                       │   Phase 3:    │
                                                       │    Polish     │
```

---

## Current State Analysis

### Existing Platform Support

| Platform | Backend | Isolation | Status |
|----------|---------|-----------|--------|
| Linux | FUSE + OverlayFS | Mount namespaces | Production |
| Windows | WinFSP | PID-based router | Production |
| macOS | None | N/A | **Not implemented** |

### Key Code References

**WinFSP Router Pattern (to adapt)**:
- `crates/spfs-vfs/src/winfsp/router.rs:32-98` - PID→Mount routing
- `crates/spfs-vfs/src/winfsp/mount.rs:39-152` - Inode management
- `crates/spfs-vfs/src/winfsp/handle.rs:12-75` - Handle types
- `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs:57-315` - CLI service/mount

**Platform Abstraction Pattern**:
- `crates/spfs/src/env_win.rs` - Windows RuntimeConfigurator
- `crates/spfs/src/status_win.rs` - Windows lifecycle
- `crates/spfs/src/runtime/storage.rs:252-305` - MountBackend enum

**Existing FUSE Implementation**:
- `crates/spfs-vfs/src/fuse.rs` - Linux FUSE filesystem (reusable logic)
- `crates/spfs-vfs/Cargo.toml:49-50` - fuser dependency

---

## Desired End State

After completing this plan:

1. **Functional macOS Support**: Users can run `spfs run <refs> -- <cmd>` on macOS (ARM64/x86_64)
2. **Per-Process Isolation**: Multiple runtimes can coexist with isolated `/spfs` views
3. **Read-Through Support**: Remote repository access works transparently
4. **Feature Parity with WinFSP**: Read-only MVP matches Windows backend capabilities

### Verification Criteria

```bash
# Phase 1 complete when:
spfs run my/package -- ls /spfs           # Shows package contents
spfs run other/pkg -- ls /spfs            # Shows different contents (isolation works)
spfs-fuse-macos service --stop            # Graceful shutdown

# Phase 2 complete when:
spfs shell --edit my/package              # Editable runtime works
spfs commit layer my/changes              # Changes can be committed

# Phase 3 complete when:
cargo build --release -p spfs-fuse-macos  # Release build on macOS CI
brew install spfs                          # (future) Homebrew installation works
```

---

## What We're NOT Doing

1. **FSKit Support**: Apple's new filesystem API lacks per-request caller context; deferred to future
2. **FUSE-T Support**: Kernel-extension-free alternative requires different integration; future consideration
3. **Overlayfs Semantics**: macOS lacks overlayfs; write support uses FUSE-based copy-on-write
4. **Monitor Process (Phase 1)**: Defer `spfs-monitor` macOS port to Phase 3
5. **Homebrew Formula**: Distribution packaging is out of scope for this plan
6. **Durable Runtimes (Phase 1)**: Focus on transient runtimes first

---

## Architecture Overview

### Service Architecture

```
┌─────────────────────────────────────────────────────────┐
│              spfs-fuse-macos service                    │
│  ┌─────────────────────────────────────────────────┐    │
│  │        macFUSE mount at /spfs                   │    │
│  │        (fuser::Session with Router)             │    │
│  └──────────────────────┬──────────────────────────┘    │
│                         │                               │
│  ┌──────────────────────▼──────────────────────────┐    │
│  │                   Router                         │    │
│  │  ┌───────────────────────────────────────────┐  │    │
│  │  │ routes: HashMap<u32, Arc<Mount>>          │  │    │
│  │  │   PID 1234 → Mount A (env: dev/base)      │  │    │
│  │  │   PID 5678 → Mount B (env: prod/tools)    │  │    │
│  │  │   default  → Empty Mount                   │  │    │
│  │  └───────────────────────────────────────────┘  │    │
│  └──────────────────────────────────────────────────┘    │
│                                                          │
│  ┌──────────────────────────────────────────────────┐    │
│  │           gRPC Service (tonic)                   │    │
│  │    - mount(root_pid, env_spec)                   │    │
│  │    - unmount(root_pid)                           │    │
│  │    - shutdown()                                  │    │
│  │    Listen: 127.0.0.1:37738                       │    │
│  └──────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

### Data Flow

```
1. Application (descendant of root_pid 1234) calls stat("/spfs/foo/bar")
         │
         ▼
2. macFUSE kernel extension intercepts, forwards to userspace
         │
         ▼
3. fuser delivers to Router::getattr(req, ino, reply)
   └── req.pid() returns application's PID (e.g., 1256)
         │
         ▼
4. Router::get_mount_for_pid(1256)
   ├── get_parent_pids_macos(1256) returns [1256, 1234, 500, 1]
   ├── routes.read() acquires lock
   ├── Search: 1256? No, 1234? YES → found Mount A
   └── Return Arc::clone(Mount A)
         │
         ▼
5. Mount A::getattr(ino, reply)
   ├── Look up inode in DashMap
   ├── Return entry attributes
   └── reply.attr(ttl, attr)
         │
         ▼
6. Result returns through fuser → macFUSE → kernel → application
```

---

## Decision Log

### Resolved Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| FUSE library | `fuser` with `macfuse-4-compat` | Already used on Linux; proven compatibility |
| Process ancestry | `libproc` crate | Native macOS API, no elevated privileges needed |
| gRPC port | 37738 | Different from WinFSP (37737) to allow testing both |
| Mount point | `/spfs` | Consistent with Linux; requires root for creation |
| Default isolation | PID-tree based | Matches WinFSP pattern; proven approach |

### Decisions to Confirm Before Implementation

| Question | Options | Recommendation | Impact |
|----------|---------|----------------|--------|
| Security model | (A) Any process can read `/spfs` (B) Verify caller UID matches runtime owner | A: Match WinFSP behavior | Low - can add later |
| Mount cleanup | (A) Periodic scan for dead PIDs (B) kqueue process exit notification (C) Lazy cleanup | B: kqueue is efficient | Medium - affects resource usage |
| Ancestry caching | (A) No cache (B) TTL-based cache (C) Per-request cache | B: 100ms TTL | Medium - affects performance |
| Read-only MVP scope | (A) MVP sufficient (B) Need write support immediately | A: Ship MVP first | High - affects timeline |

---

## Phase 1: Read-Only MVP (4-6 weeks)

### Overview
Establish a working macFUSE mount with PID-based routing, enabling read-only access to SPFS environments on macOS.

### Milestone
Users can run `spfs run <refs> -- <cmd>` on macOS and see the correct filesystem view based on their process ancestry.

---

### Task 1.1: Project Structure Setup

**Effort**: 1-2 days  
**Dependencies**: None

**Files to Create**:
```
crates/spfs-vfs/src/macos/
  mod.rs
  router.rs      (placeholder)
  mount.rs       (placeholder)
  process.rs     (placeholder)

crates/spfs-cli/cmd-fuse-macos/
  Cargo.toml
  src/
    main.rs
    cmd_fuse_macos.rs
```

**Changes to `crates/spfs-vfs/Cargo.toml`**:
```toml
[features]
default = []
winfsp-backend = [...]
fuse-backend = [...]
macfuse-backend = [
    "dep:tonic",
    "dep:prost",
    "dep:futures-core",
    "dep:fuser",
    "dep:libproc",
]

[target.'cfg(target_os = "macos")'.dependencies]
fuser = { workspace = true, optional = true, features = ["macfuse-4-compat"] }
libproc = { version = "0.14", optional = true }
```

**Changes to `crates/spfs-vfs/src/lib.rs`**:
```rust
#[cfg(all(target_os = "macos", feature = "macfuse-backend"))]
pub mod macos;
#[cfg(all(target_os = "macos", feature = "macfuse-backend"))]
pub mod proto;  // Reuse existing proto
#[cfg(all(target_os = "macos", feature = "macfuse-backend"))]
pub use macos::{Config, Service};
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend --target aarch64-apple-darwin` passes
- [ ] `cargo check -p spfs-vfs --features macfuse-backend --target x86_64-apple-darwin` passes
- [ ] New module structure exists with placeholder files
- [ ] Linux/Windows builds unaffected: `cargo check -p spfs-vfs` passes on Linux

#### Manual Verification:
- [ ] Directory structure matches plan

---

### Task 1.2: Process Ancestry Tracking

**Effort**: 2-3 days  
**Dependencies**: Task 1.1

**File**: `crates/spfs-vfs/src/macos/process.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS process ancestry tracking using libproc

use libproc::libproc::proc_pid::pidinfo;
use libproc::libproc::bsd_info::BSDInfo;

/// Error type for process ancestry operations
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("Failed to get process info for PID {pid}: {message}")]
    InfoError { pid: i32, message: String },
}

/// Get the process ancestry chain from a given PID up to launchd (PID 1)
/// 
/// Returns a vector starting with the given PID, followed by its parent,
/// grandparent, etc., up to PID 1 (launchd).
/// 
/// # Example
/// ```ignore
/// // For a process tree: 1 -> 500 -> 1234 -> 1256
/// let ancestry = get_parent_pids_macos(Some(1256))?;
/// assert_eq!(ancestry, vec![1256, 1234, 500, 1]);
/// ```
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>, ProcessError> {
    let mut current = match root {
        Some(pid) => pid,
        None => std::process::id() as i32,
    };
    
    let mut stack = vec![current];
    const MAX_DEPTH: usize = 100;
    
    for _ in 0..MAX_DEPTH {
        let info: BSDInfo = pidinfo(current, 0)
            .map_err(|e| ProcessError::InfoError { 
                pid: current, 
                message: e.to_string() 
            })?;
        
        let parent = info.pbi_ppid as i32;
        
        // Stop at launchd (PID 1) or if parent == self (orphan)
        if parent == 0 || parent == current || current == 1 {
            break;
        }
        
        stack.push(parent);
        current = parent;
    }
    
    Ok(stack)
}

/// Check if caller_pid is a descendant of root_pid
pub fn is_in_process_tree(caller_pid: i32, root_pid: i32) -> bool {
    match get_parent_pids_macos(Some(caller_pid)) {
        Ok(ancestry) => ancestry.contains(&root_pid),
        Err(_) => false,
    }
}

/// Get the parent PID of the current process
pub fn get_parent_pid() -> Result<u32, ProcessError> {
    let ancestry = get_parent_pids_macos(None)?;
    ancestry.get(1)
        .map(|&pid| pid as u32)
        .ok_or_else(|| ProcessError::InfoError {
            pid: std::process::id() as i32,
            message: "No parent process found".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_current_process_ancestry() {
        let ancestry = get_parent_pids_macos(None).unwrap();
        assert!(!ancestry.is_empty());
        assert_eq!(ancestry[0], std::process::id() as i32);
    }
    
    #[test]
    fn test_ancestry_reaches_launchd() {
        let ancestry = get_parent_pids_macos(None).unwrap();
        // Should eventually reach PID 1 (launchd) or stop at MAX_DEPTH
        let last = *ancestry.last().unwrap();
        assert!(last == 1 || ancestry.len() == 100);
    }
    
    #[test]
    fn test_is_in_process_tree_self() {
        let pid = std::process::id() as i32;
        assert!(is_in_process_tree(pid, pid));
    }
    
    #[test]
    fn test_is_in_process_tree_parent() {
        let ancestry = get_parent_pids_macos(None).unwrap();
        if ancestry.len() > 1 {
            let current = ancestry[0];
            let parent = ancestry[1];
            assert!(is_in_process_tree(current, parent));
        }
    }
    
    #[test]
    fn test_get_parent_pid() {
        let parent = get_parent_pid().unwrap();
        assert!(parent > 0);
    }
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] Unit tests pass: `cargo test -p spfs-vfs --features macfuse-backend -- macos::process`
- [ ] No unsafe code warnings
- [ ] Documentation compiles: `cargo doc -p spfs-vfs --features macfuse-backend`

#### Manual Verification:
- [ ] Test on both ARM64 and x86_64 Mac hardware
- [ ] Verify ancestry chain is correct using `pstree` comparison

---

### Task 1.3: Handle Types Implementation

**Effort**: 1 day  
**Dependencies**: Task 1.1

**File**: `crates/spfs-vfs/src/macos/handle.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! File handle types for macOS FUSE filesystem

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use spfs::tracking::{BlobRead, Entry};

/// A handle to a file or directory in the spfs runtime
pub enum Handle {
    /// A handle to a real file on disk that can be seek'd
    BlobFile {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
        /// The on-disk file containing this blob data
        file: std::fs::File,
    },
    /// A handle to an opaque file stream that can only be read once
    BlobStream {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
        /// The current offset of the file stream
        ///
        /// Streams cannot be seek'd and must be read through contiguously
        /// and only once. This value is used to ensure that reads do not
        /// attempt to move the offset.
        offset: Arc<AtomicU64>,
        /// The opaque data stream for this blob
        stream: Arc<tokio::sync::Mutex<Pin<Box<dyn BlobRead>>>>,
    },
    /// A handle to an open directory that can be read
    Tree {
        /// The underlying entry data for this filesystem node
        entry: Arc<Entry<u64>>,
    },
}

impl std::fmt::Debug for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BlobFile { entry, .. } => f
                .debug_struct("BlobFile")
                .field("ino", &entry.user_data)
                .finish(),
            Self::BlobStream { entry, .. } => f
                .debug_struct("BlobStream")
                .field("ino", &entry.user_data)
                .finish(),
            Self::Tree { entry } => f
                .debug_struct("Tree")
                .field("ino", &entry.user_data)
                .finish(),
        }
    }
}

impl Handle {
    /// The allocated inode value for this handle
    pub fn ino(&self) -> u64 {
        match self {
            Self::BlobFile { entry, .. } => entry.user_data,
            Self::BlobStream { entry, .. } => entry.user_data,
            Self::Tree { entry } => entry.user_data,
        }
    }

    /// An unowned reference to the entry data of this handle
    pub fn entry(&self) -> &Entry<u64> {
        match self {
            Self::BlobFile { entry, .. } => entry,
            Self::BlobStream { entry, .. } => entry,
            Self::Tree { entry, .. } => entry,
        }
    }

    /// An owned reference to the entry data of this handle
    pub fn entry_owned(&self) -> Arc<Entry<u64>> {
        match self {
            Self::BlobFile { entry, .. } => Arc::clone(entry),
            Self::BlobStream { entry, .. } => Arc::clone(entry),
            Self::Tree { entry, .. } => Arc::clone(entry),
        }
    }
    
    /// Returns true if this handle is for a directory
    pub fn is_dir(&self) -> bool {
        matches!(self, Self::Tree { .. })
    }
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Types match WinFSP handle.rs structure

---

### Task 1.4: Mount Implementation

**Effort**: 3-4 days  
**Dependencies**: Task 1.2, Task 1.3

**File**: `crates/spfs-vfs/src/macos/mount.rs`

This is a substantial file - port from `winfsp/mount.rs` with these adaptations:
- Replace Windows file attribute types with POSIX mode bits
- Replace Windows time types with `SystemTime`
- Adapt path handling from Windows (`\\`) to POSIX (`/`)
- Use fuser reply types instead of winfsp types

Key methods to implement:
- `Mount::new()` - Create mount from manifest
- `Mount::empty()` - Create empty default mount  
- `allocate_inodes()` - Pre-allocate inode tree
- `attr_from_entry()` - Convert Entry to FileAttr
- `lookup()` - Find child by name
- `getattr()` - Get file attributes
- `open()` - Open file handle
- `read()` - Read file data
- `readdir()` - List directory contents
- `readlink()` - Read symlink target

**Source Reference**: `crates/spfs-vfs/src/winfsp/mount.rs:39-634`

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Unit tests for inode allocation pass
- [ ] Unit tests for attr conversion pass

#### Manual Verification:
- [ ] Code review confirms all WinFSP mount functionality is ported

---

### Task 1.5: Router Implementation

**Effort**: 3-4 days  
**Dependencies**: Task 1.4

**File**: `crates/spfs-vfs/src/macos/router.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! PID-based filesystem router for macOS
//! 
//! Routes filesystem operations to per-runtime Mount instances based on
//! the calling process's ancestry chain.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory,
    ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request,
};
use spfs::tracking::EnvSpec;
use tracing::instrument;

use super::mount::Mount;
use super::handle::Handle;
use super::process::get_parent_pids_macos;

/// Routes filesystem operations based on calling process PID
/// 
/// The router maintains a mapping of root PIDs to Mount instances.
/// When a filesystem operation arrives, it determines which Mount
/// to use by walking up the caller's process ancestry until it
/// finds a registered root PID.
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    /// PID -> Mount mapping
    /// TODO: Consider DashMap for lock-free reads
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,
    /// Default mount for unregistered processes (empty filesystem)
    default: Arc<Mount>,
}

impl Router {
    /// Construct an empty router with no mounted filesystem views
    pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
        let default = Arc::new(Mount::empty()?);
        Ok(Self {
            repos,
            routes: Arc::new(RwLock::new(HashMap::default())),
            default,
        })
    }

    /// Add a new mount for the given process ID and all its descendants
    #[instrument(skip(self))]
    pub async fn mount(&self, root_pid: u32, env_spec: EnvSpec) -> spfs::Result<()> {
        tracing::debug!("Computing environment manifest...");
        let mut manifest = Err(spfs::Error::UnknownReference(env_spec.to_string()));
        for repo in self.repos.iter() {
            manifest = spfs::compute_environment_manifest(&env_spec, repo).await;
            if manifest.is_ok() {
                break;
            }
        }
        let manifest = manifest?;
        let rt = tokio::runtime::Handle::current();
        let mount = Mount::new(rt, self.repos.clone(), manifest)?;
        
        tracing::info!(%root_pid, env_spec=%env_spec.to_string(), "mounted");
        let mut routes = self.routes.write().expect("lock is never poisoned");
        if routes.contains_key(&root_pid) {
            return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
        }
        routes.insert(root_pid, Arc::new(mount));
        Ok(())
    }

    /// Remove a mount for the given root PID
    #[instrument(skip(self))]
    pub fn unmount(&self, root_pid: u32) -> bool {
        let mut routes = self.routes.write().expect("lock is never poisoned");
        routes.remove(&root_pid).is_some()
    }

    /// Get the mount for a calling process by walking its ancestry
    fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
        let stack = get_parent_pids_macos(Some(caller_pid as i32))
            .unwrap_or_else(|_| vec![caller_pid as i32]);
        
        let routes = self.routes.read().expect("lock is never poisoned");
        for pid in stack {
            if let Some(mount) = routes.get(&(pid as u32)) {
                return Arc::clone(mount);
            }
        }
        Arc::clone(&self.default)
    }
    
    /// Number of active mounts
    pub fn mount_count(&self) -> usize {
        self.routes.read().expect("lock is never poisoned").len()
    }
}

impl Filesystem for Router {
    fn init(
        &mut self,
        _req: &Request<'_>,
        _config: &mut fuser::KernelConfig,
    ) -> Result<(), libc::c_int> {
        tracing::info!("FUSE filesystem initialized");
        Ok(())
    }

    fn destroy(&mut self) {
        tracing::info!("FUSE filesystem destroyed");
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn lookup(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.lookup(parent, name, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn getattr(&mut self, req: &Request<'_>, ino: u64, reply: ReplyAttr) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.getattr(ino, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn read(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        size: u32,
        flags: i32,
        lock_owner: Option<u64>,
        reply: ReplyData,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.read(ino, fh, offset, size, flags, lock_owner, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn readdir(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        offset: i64,
        reply: ReplyDirectory,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.readdir(ino, fh, offset, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn open(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.open(ino, flags, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn opendir(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.opendir(ino, flags, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn readlink(&mut self, req: &Request<'_>, ino: u64, reply: ReplyData) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.readlink(ino, reply);
    }

    #[instrument(skip(self, reply), fields(pid = req.pid()))]
    fn statfs(&mut self, req: &Request<'_>, ino: u64, reply: ReplyStatfs) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.statfs(ino, reply);
    }

    fn release(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        lock_owner: Option<u64>,
        flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.release(ino, fh, flags, lock_owner, flush, reply);
    }

    fn releasedir(
        &mut self,
        req: &Request<'_>,
        ino: u64,
        fh: u64,
        flags: i32,
        reply: fuser::ReplyEmpty,
    ) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.releasedir(ino, fh, flags, reply);
    }

    fn access(&mut self, req: &Request<'_>, ino: u64, mask: i32, reply: fuser::ReplyEmpty) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.access(ino, mask, reply);
    }
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] Unit tests for routing logic pass
- [ ] All fuser::Filesystem methods delegate to mount

#### Manual Verification:
- [ ] Code structure matches WinFSP router pattern

---

### Task 1.6: Module Exports

**Effort**: 0.5 days  
**Dependencies**: Task 1.3, Task 1.4, Task 1.5

**File**: `crates/spfs-vfs/src/macos/mod.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS-specific virtual filesystem implementation using macFUSE
//!
//! This module provides SPFS filesystem support on macOS using the macFUSE
//! kernel extension. Unlike Linux which uses mount namespaces for isolation,
//! macOS uses a WinFSP-style router that maps process IDs to filesystem views.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────┐
//! │   spfs-fuse-macos       │
//! │   service               │
//! │   ┌─────────────────┐   │
//! │   │ macFUSE mount   │   │
//! │   │ at /spfs        │   │
//! │   └────────┬────────┘   │
//! │            │            │
//! │   ┌────────▼────────┐   │
//! │   │     Router      │   │
//! │   │  PID → Mount    │   │
//! │   └────────┬────────┘   │
//! │            │            │
//! │   ┌────────▼────────┐   │
//! │   │  gRPC Service   │   │
//! │   │  (tonic)        │   │
//! │   └─────────────────┘   │
//! └─────────────────────────┘
//! ```

mod handle;
mod mount;
mod process;
mod router;

pub use handle::Handle;
pub use mount::Mount;
pub use process::{get_parent_pids_macos, get_parent_pid, is_in_process_tree, ProcessError};
pub use router::Router;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use fuser::MountOption;
use spfs::storage::FromConfig;
use tonic::{Request, Response, Status, async_trait};
use tracing::instrument;

use crate::proto;

/// Configuration for the macOS FUSE service
#[derive(Debug, Clone)]
pub struct Config {
    /// The location on the host where SPFS will be mounted
    pub mountpoint: PathBuf,
    /// Remote repositories that can be read from
    pub remotes: Vec<String>,
    /// FUSE mount options
    pub mount_options: HashSet<MountOption>,
}

impl Default for Config {
    fn default() -> Self {
        let mut mount_options = HashSet::new();
        mount_options.insert(MountOption::RO);
        mount_options.insert(MountOption::FSName("spfs".to_string()));
        mount_options.insert(MountOption::AllowOther);
        
        Self {
            mountpoint: PathBuf::from("/spfs"),
            remotes: Vec::new(),
            mount_options,
        }
    }
}

/// The macOS FUSE service that manages the filesystem mount and gRPC control plane
pub struct Service {
    config: Config,
    router: Router,
    session: Option<fuser::BackgroundSession>,
}

impl Service {
    /// Create a new service with the provided configuration
    pub async fn new(config: Config) -> spfs::Result<Arc<Self>> {
        let spfs_config = spfs::Config::current()?;
        tracing::debug!("Opening repositories...");
        
        let proxy_config = spfs::storage::proxy::Config {
            primary: format!(
                "file://{}?create=true",
                spfs_config.storage.root.to_string_lossy()
            ),
            secondary: config.remotes.clone(),
        };
        let repo = spfs::storage::ProxyRepository::from_config(proxy_config)
            .await
            .map_err(|source| spfs::Error::FailedToOpenRepository {
                repository: "<macFUSE Repository Stack>".into(),
                source,
            })?;
        let repos = repo.into_stack().into_iter().map(Arc::new).collect();
        
        let router = Router::new(repos).await?;
        
        Ok(Arc::new(Self {
            config,
            router,
            session: None,
        }))
    }
    
    /// Start the FUSE mount
    pub fn start_mount(&mut self) -> spfs::Result<()> {
        let options: Vec<MountOption> = self.config.mount_options.iter().cloned().collect();
        
        let session = fuser::spawn_mount2(
            self.router.clone(),
            &self.config.mountpoint,
            &options,
        ).map_err(|e| spfs::Error::String(format!("Failed to mount FUSE: {}", e)))?;
        
        self.session = Some(session);
        tracing::info!(mountpoint = %self.config.mountpoint.display(), "FUSE mount started");
        Ok(())
    }
    
    /// Stop the FUSE mount
    pub fn stop_mount(&mut self) {
        if let Some(session) = self.session.take() {
            drop(session);
            tracing::info!("FUSE mount stopped");
        }
    }
    
    /// Get a reference to the router for gRPC service implementation
    pub fn router(&self) -> &Router {
        &self.router
    }
}

#[async_trait]
impl proto::vfs_service_server::VfsService for Arc<Service> {
    #[instrument(skip_all)]
    async fn shutdown(
        &self,
        _request: Request<proto::ShutdownRequest>,
    ) -> std::result::Result<Response<proto::ShutdownResponse>, Status> {
        tracing::debug!("Shutdown request received");
        // Note: actual shutdown is handled by the CLI service loop
        Ok(Response::new(proto::ShutdownResponse {}))
    }

    #[instrument(skip_all)]
    async fn mount(
        &self,
        request: Request<proto::MountRequest>,
    ) -> std::result::Result<Response<proto::MountResponse>, Status> {
        tracing::debug!("Mount request received");
        let inner = request.into_inner();
        let env_spec = spfs::tracking::EnvSpec::parse(&inner.env_spec).map_err(|err| {
            Status::invalid_argument(format!("Provided env spec was invalid: {err}"))
        })?;
        if let Err(err) = self.router.mount(inner.root_pid, env_spec).await {
            tracing::error!("{err}");
            return Err(Status::internal(format!(
                "Failed to mount filesystem: {err}"
            )));
        }
        Ok(Response::new(proto::MountResponse {}))
    }
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] `cargo doc -p spfs-vfs --features macfuse-backend` generates docs

---

### Task 1.7: CLI Binary Implementation

**Effort**: 2-3 days  
**Dependencies**: Task 1.6

**File**: `crates/spfs-cli/cmd-fuse-macos/Cargo.toml`

```toml
[package]
name = "spfs-cli-fuse-macos"
version.workspace = true
authors.workspace = true
edition.workspace = true
license-file.workspace = true

[[bin]]
name = "spfs-fuse-macos"
path = "src/main.rs"

[dependencies]
clap = { workspace = true }
miette = { workspace = true, features = ["fancy"] }
spfs = { workspace = true }
spfs-cli-common = { workspace = true }
spfs-vfs = { workspace = true, features = ["macfuse-backend"] }
tokio = { workspace = true, features = ["rt", "rt-multi-thread", "signal"] }
tonic = { workspace = true }
tracing = { workspace = true }
```

**File**: `crates/spfs-cli/cmd-fuse-macos/src/main.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

fn main() -> miette::Result<()> {
    std::process::exit(cmd_fuse_macos::main()?)
}

mod cmd_fuse_macos;
```

**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::net::SocketAddr;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand};
use miette::{Context, IntoDiagnostic, Result, bail};
use spfs::tracking::EnvSpec;
use spfs_cli_common as cli;
use spfs_vfs::{proto, Service, Config};
use spfs_vfs::macos::get_parent_pid;
use tonic::Request;

pub fn main() -> Result<i32> {
    let mut opt = CmdFuseMacos::parse();
    opt.logging.syslog = true;
    unsafe { opt.logging.configure(); }

    let config = match spfs::get_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return Ok(1);
        }
        Ok(config) => config,
    };

    let result = opt.run(&config);
    spfs_cli_common::handle_result!(result)
}

/// Run a virtual filesystem backed by macFUSE
#[derive(Debug, Parser)]
#[clap(name = "spfs-fuse-macos")]
pub struct CmdFuseMacos {
    #[clap(flatten)]
    logging: cli::Logging,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Service(CmdService),
    Mount(CmdMount),
}

impl cli::CommandName for CmdFuseMacos {
    fn command_name(&self) -> &str {
        "fuse-macos"
    }
}

impl CmdFuseMacos {
    fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .into_diagnostic()
            .wrap_err("Failed to establish async runtime")?;
        let res = match &mut self.command {
            Command::Mount(c) => rt.block_on(c.run(config)),
            Command::Service(c) => rt.block_on(c.run(config)),
        };
        rt.shutdown_timeout(std::time::Duration::from_secs(30));
        res
    }
}

/// Start the background filesystem service
#[derive(Debug, Args)]
struct CmdService {
    /// Stop the running service instead of starting it
    #[clap(long, exclusive = true)]
    stop: bool,

    /// The local address to listen on for filesystem control
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    listen: SocketAddr,

    /// The location where to mount the spfs runtime
    #[clap(default_value = "/spfs")]
    mountpoint: std::path::PathBuf,
}

impl CmdService {
    async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        if self.stop {
            return self.stop().await;
        }

        tracing::info!("Starting macFUSE service...");
        
        let vfs_config = Config {
            mountpoint: self.mountpoint.clone(),
            remotes: config.filesystem.secondary_repositories.clone(),
            ..Default::default()
        };
        
        let mut service = Service::new(vfs_config)
            .await
            .into_diagnostic()
            .wrap_err("Failed to create service")?;
        
        // Start the FUSE mount
        Arc::get_mut(&mut service)
            .expect("sole reference")
            .start_mount()
            .into_diagnostic()
            .wrap_err("Failed to start FUSE mount")?;
        
        // Set up shutdown handling
        let (shutdown_tx, mut shutdown_rx) = tokio::sync::mpsc::channel(4);
        let ctrl_c_shutdown = shutdown_tx.clone();
        tokio::task::spawn(async move {
            if let Err(err) = tokio::signal::ctrl_c().await {
                tracing::error!(?err, "Failed to setup graceful shutdown handler");
            };
            let _ = ctrl_c_shutdown.send(()).await;
        });
        
        // Start gRPC server
        let grpc_service = proto::vfs_service_server::VfsServiceServer::new(Arc::clone(&service));
        let server = tonic::transport::Server::builder()
            .add_service(grpc_service)
            .serve_with_shutdown(self.listen, async {
                let _ = shutdown_rx.recv().await;
                tracing::info!("Shutting down gRPC server...");
            });
        
        tracing::info!(listen = %self.listen, mountpoint = %self.mountpoint.display(), "Service started");
        
        server.await.into_diagnostic().wrap_err("gRPC server failed")?;
        
        tracing::info!("Service stopped");
        Ok(0)
    }

    async fn stop(&self) -> Result<i32> {
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{}", self.listen))
            .into_diagnostic()
            .wrap_err("Invalid server address")?
            .connect_lazy();
        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);
        let res = client
            .shutdown(tonic::Request::new(proto::ShutdownRequest {}))
            .await;
        match res {
            Ok(_) => {
                tracing::info!("Stop request accepted");
                Ok(0)
            }
            Err(err) if is_connection_refused(&err) => {
                tracing::warn!(addr=%self.listen, "The service does not appear to be running");
                Ok(0)
            }
            Err(err) => Err(err).into_diagnostic(),
        }
    }
}

/// Request a mount for a specific process tree
#[derive(Debug, Args)]
struct CmdMount {
    /// The process id for which the mount will be visible
    #[clap(long)]
    root_process: Option<u32>,

    /// The local address to connect to for filesystem control
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    service: SocketAddr,

    /// The tag or id of the files to mount
    #[clap(name = "REF")]
    reference: EnvSpec,
}

impl CmdMount {
    async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        let result = tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))
            .into_diagnostic()
            .wrap_err("Invalid server address")?
            .connect()
            .await;
        
        let channel = match result {
            Err(err) if is_connection_refused(&err) => {
                bail!("Service is not running. Start it with: spfs-fuse-macos service");
            }
            res => res.into_diagnostic()?,
        };

        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);

        let root_pid = match self.root_process {
            Some(pid) => pid,
            None => get_parent_pid().into_diagnostic()?,
        };
        
        client
            .mount(Request::new(proto::MountRequest {
                root_pid,
                env_spec: self.reference.to_string(),
            }))
            .await
            .into_diagnostic()
            .wrap_err("Failed to mount filesystem")?;

        tracing::info!(root_pid, env_spec = %self.reference, "Mount registered");
        Ok(0)
    }
}

fn is_connection_refused<T: std::error::Error>(err: &T) -> bool {
    let Some(mut source) = err.source() else {
        return false;
    };
    while let Some(src) = source.source() {
        source = src;
    }
    if let Some(io_err) = source.downcast_ref::<std::io::Error>() {
        return io_err.kind() == std::io::ErrorKind::ConnectionRefused;
    }
    false
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo build -p spfs-cli-fuse-macos` succeeds on macOS
- [ ] Binary exists at `target/debug/spfs-fuse-macos`
- [ ] `spfs-fuse-macos --help` shows usage

#### Manual Verification:
- [ ] `spfs-fuse-macos service` starts without error (requires macFUSE installed)
- [ ] `spfs-fuse-macos service --stop` stops a running service
- [ ] `spfs-fuse-macos mount <ref>` registers a mount

---

### Task 1.8: MountBackend Enum Update

**Effort**: 0.5 days  
**Dependencies**: Task 1.1

**File**: `crates/spfs/src/runtime/storage.rs`

**Changes** (around line 252):

```rust
/// Identifies a filesystem backend for spfs
#[derive(
    Default,
    Clone,
    Copy,
    Debug,
    Eq,
    PartialEq,
    strum::Display,
    strum::EnumString,
    strum::VariantNames,
    Serialize,
    Deserialize,
)]
pub enum MountBackend {
    /// Renders each layer to a folder on disk, before mounting
    /// the whole stack as lower directories in overlayfs. Edits
    /// are stored in the overlayfs upper directory.
    #[cfg_attr(all(unix, not(target_os = "macos")), default)]
    OverlayFsWithRenders,
    /// Mounts a fuse filesystem as the lower directory to
    /// overlayfs, using the overlayfs upper directory for edits
    OverlayFsWithFuse,
    /// Mounts a fuse filesystem directly
    FuseOnly,
    /// Leverages the win file system protocol system to present
    /// dynamic file system entries to runtime processes
    #[cfg_attr(windows, default)]
    WinFsp,
    /// Uses macFUSE with PID-based routing for process isolation
    #[cfg_attr(target_os = "macos", default)]
    MacFuse,
}

impl MountBackend {
    // ... existing methods ...

    pub fn is_macfuse(&self) -> bool {
        matches!(self, Self::MacFuse)
    }

    pub fn is_fuse(&self) -> bool {
        match self {
            MountBackend::OverlayFsWithRenders => false,
            MountBackend::OverlayFsWithFuse => true,
            MountBackend::FuseOnly => true,
            MountBackend::WinFsp => false,
            MountBackend::MacFuse => true,
        }
    }

    pub fn requires_localization(&self) -> bool {
        match self {
            Self::OverlayFsWithRenders => true,
            Self::OverlayFsWithFuse => false,
            Self::FuseOnly => false,
            Self::WinFsp => false,
            Self::MacFuse => false,
        }
    }
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs` passes on all platforms
- [ ] Existing tests pass
- [ ] `MacFuse` is default on macOS builds

---

### Task 1.9: macOS RuntimeConfigurator

**Effort**: 1-2 days  
**Dependencies**: Task 1.7, Task 1.8

**File**: `crates/spfs/src/env_macos.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS-specific runtime environment configuration

use crate::tracking::EnvSpec;
use crate::{Error, Result, runtime};

pub const SPFS_DIR: &str = "/spfs";
pub const SPFS_DIR_PREFIX: &str = "/spfs";

/// Manages the configuration of an spfs runtime environment on macOS.
#[derive(Default)]
pub struct RuntimeConfigurator;

impl RuntimeConfigurator {
    /// Make this configurator for an existing runtime.
    pub fn current_runtime(self, _rt: &runtime::Runtime) -> Result<Self> {
        // macOS doesn't use namespaces, so this is a no-op
        Ok(self)
    }

    /// Move this process into the namespace of an existing runtime
    pub fn join_runtime(self, _rt: &runtime::Runtime) -> Result<Self> {
        // macOS doesn't have mount namespaces
        // The router handles isolation via PID-based routing
        Ok(self)
    }

    /// Mount the provided runtime via the macFUSE backend
    pub async fn mount_env_macfuse(&self, rt: &runtime::Runtime) -> Result<()> {
        let Some(root_pid) = rt.status.owner else {
            return Err(Error::RuntimeNotInitialized(
                "Missing owner in runtime, cannot initialize".to_string(),
            ));
        };

        let env_spec = rt
            .status
            .stack
            .iter_bottom_up()
            .collect::<EnvSpec>()
            .to_string();

        let exe = crate::which_spfs("fuse-macos")
            .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;
        
        let mut cmd = tokio::process::Command::new(exe);
        cmd.arg("mount")
            .arg("--root-process")
            .arg(root_pid.to_string())
            .arg(env_spec);
        
        tracing::debug!("{cmd:?}");
        let status = cmd.status().await;
        
        match status {
            Err(err) => Err(Error::process_spawn_error("spfs-fuse-macos", err, None)),
            Ok(st) if st.success() => Ok(()),
            Ok(st) => Err(Error::String(format!(
                "Failed to mount macFUSE filesystem, mount command exited with non-zero status {:?}",
                st.code()
            ))),
        }
    }
}

/// Unmount a FUSE filesystem on macOS
pub fn unmount_fuse(path: &std::path::Path) -> Result<()> {
    // Try umount first, then diskutil as fallback
    let status = std::process::Command::new("umount")
        .arg(path)
        .status();
    
    match status {
        Ok(s) if s.success() => return Ok(()),
        _ => {}
    }
    
    // Fallback to diskutil
    let status = std::process::Command::new("diskutil")
        .arg("unmount")
        .arg(path)
        .status()
        .map_err(|e| Error::String(format!("Failed to unmount {}: {}", path.display(), e)))?;
    
    if status.success() {
        Ok(())
    } else {
        Err(Error::String(format!(
            "Failed to unmount {}: exit code {:?}",
            path.display(),
            status.code()
        )))
    }
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs` passes on macOS
- [ ] Pattern matches `env_win.rs` structure

---

### Task 1.10: macOS Status Module

**Effort**: 1 day  
**Dependencies**: Task 1.9

**File**: `crates/spfs/src/status_macos.rs`

```rust
// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

//! macOS runtime lifecycle management

use crate::storage::fs::RenderSummary;
use crate::{Error, Result, env, runtime};

/// Remount the given runtime as configured.
pub async fn remount_runtime(_rt: &runtime::Runtime) -> Result<()> {
    // TODO: Implement remount for macOS
    Err(Error::String("Remount not yet implemented on macOS".to_string()))
}

/// Exit the given runtime as configured
pub async fn exit_runtime(_rt: &runtime::Runtime) -> Result<()> {
    // TODO: Implement runtime exit cleanup
    // This should unregister the mount from the router
    Ok(())
}

/// Turn the given runtime into a durable runtime
pub async fn make_runtime_durable(_rt: &runtime::Runtime) -> Result<()> {
    Err(Error::String("Durable runtimes not yet supported on macOS".to_string()))
}

/// Reinitialize the current spfs runtime as a durable runtime
pub async fn change_to_durable_runtime(_rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    Err(Error::String("Durable runtimes not yet supported on macOS".to_string()))
}

/// Reinitialize the current spfs runtime
pub async fn reinitialize_runtime(_rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    Err(Error::String("Reinitialize not yet implemented on macOS".to_string()))
}

/// Initialize the current runtime
pub async fn initialize_runtime(rt: &mut runtime::Runtime) -> Result<RenderSummary> {
    tracing::debug!("computing runtime manifest");
    let _manifest = super::compute_runtime_manifest(rt).await?;

    let configurator = env::RuntimeConfigurator;
    match rt.config.mount_backend {
        #[cfg(feature = "macfuse-backend")]
        runtime::MountBackend::MacFuse => {
            configurator.mount_env_macfuse(rt).await?;
        }
        #[allow(unreachable_patterns)]
        _ => {
            return Err(Error::String(format!(
                "This binary was not compiled with support for {}",
                rt.config.mount_backend
            )));
        }
    }
    Ok(RenderSummary::default())
}
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs --features macfuse-backend` passes
- [ ] Structure matches `status_win.rs`

---

### Task 1.11: Platform Module Switching

**Effort**: 0.5 days  
**Dependencies**: Task 1.9, Task 1.10

**File**: `crates/spfs/src/lib.rs`

Add conditional compilation for macOS modules:

```rust
// Around line 50, add:
#[cfg_attr(target_os = "macos", path = "./env_macos.rs")]
#[cfg_attr(windows, path = "./env_win.rs")]
#[cfg_attr(all(unix, not(target_os = "macos")), path = "./env.rs")]
pub mod env;

#[cfg_attr(target_os = "macos", path = "./status_macos.rs")]
#[cfg_attr(windows, path = "./status_win.rs")]
#[cfg_attr(all(unix, not(target_os = "macos")), path = "./status_unix.rs")]
mod status;
```

**Acceptance Criteria**:

#### Automated Verification:
- [ ] `cargo check -p spfs` passes on Linux (unchanged)
- [ ] `cargo check -p spfs` passes on Windows (unchanged)
- [ ] `cargo check -p spfs --features macfuse-backend` passes on macOS

---

### Task 1.12: Integration Testing

**Effort**: 3-4 days  
**Dependencies**: All previous Phase 1 tasks

**File**: `crates/spfs-vfs/src/macos/tests.rs` (new integration tests)

**Test Scenarios**:

1. **Process Ancestry Tests**
   - Verify ancestry chain is correct
   - Test orphaned process handling
   - Test process tree depth limits

2. **Router Tests**
   - Mount registers correctly
   - PID routing finds correct mount
   - Default mount used for unregistered processes
   - Multiple mounts coexist

3. **Mount Tests**
   - Inode allocation is correct
   - File attributes are accurate
   - Directory listing works
   - File reading works
   - Symlinks resolve correctly

4. **End-to-End Tests** (require macFUSE)
   - Service starts and stops cleanly
   - Mount command registers with service
   - File operations work through FUSE
   - Process isolation is effective

**Acceptance Criteria**:

#### Automated Verification:
- [ ] Unit tests pass: `cargo test -p spfs-vfs --features macfuse-backend`
- [ ] Integration tests pass on macOS CI runner

#### Manual Verification:
- [ ] Full end-to-end test on macOS ARM64
- [ ] Full end-to-end test on macOS x86_64
- [ ] Test with multiple concurrent runtimes

---

### Phase 1 Success Criteria Summary

#### Automated Verification:
- [ ] All crates compile on macOS: `cargo build --workspace`
- [ ] All unit tests pass: `cargo test --workspace`
- [ ] Linux/Windows builds unaffected
- [ ] macOS CI job passes (if configured)

#### Manual Verification:
- [ ] `spfs-fuse-macos service` starts on macOS with macFUSE installed
- [ ] `spfs run <ref> -- ls /spfs` shows expected files
- [ ] Two concurrent `spfs run` commands see isolated views
- [ ] Service shutdown is clean with no zombie mounts
- [ ] Works on both ARM64 and x86_64 Macs

---

## Phase 2: Write Support (4-6 weeks)

### Overview
Add copy-on-write semantics to enable editable runtimes on macOS, implementing FUSE write operations with a scratch directory.

### Milestone
Users can run `spfs shell --edit <refs>` and make changes that can be committed.

---

### Task 2.1: Scratch Directory Management

**Effort**: 2-3 days  
**Dependencies**: Phase 1 complete

**File**: `crates/spfs-vfs/src/macos/scratch.rs`

Implement a scratch directory that stores modified files:
- Create temp directory per mount
- Track modified files
- Track deleted files (whiteouts)
- Clean up on unmount

---

### Task 2.2: FUSE Write Operations

**Effort**: 4-5 days  
**Dependencies**: Task 2.1

**Changes to**: `crates/spfs-vfs/src/macos/mount.rs`

Implement these fuser::Filesystem methods:
- `write()` - Write data to file (copy-up from repo if needed)
- `create()` - Create new file
- `mkdir()` - Create directory
- `unlink()` - Delete file (whiteout)
- `rmdir()` - Delete directory
- `rename()` - Rename file/directory
- `setattr()` - Change file attributes
- `truncate()` - Truncate file

---

### Task 2.3: Copy-Up Semantics

**Effort**: 3-4 days  
**Dependencies**: Task 2.2

Implement copy-on-write:
- On first write, copy file from repo to scratch
- Track which files have been copied
- Handle partial writes correctly

---

### Task 2.4: Whiteout Tracking

**Effort**: 2-3 days  
**Dependencies**: Task 2.2

Track deletions:
- Store whiteout markers for deleted files
- Hide deleted files in directory listings
- Handle deletion of modified files

---

### Task 2.5: Edit Mode Integration

**Effort**: 3-4 days  
**Dependencies**: Task 2.3, Task 2.4

Update `status_macos.rs` to support:
- Editable runtime initialization
- Commit changes from scratch directory
- Clean up scratch on exit

---

### Phase 2 Success Criteria

#### Automated Verification:
- [ ] Write operation tests pass
- [ ] Copy-up tests pass
- [ ] Whiteout tests pass

#### Manual Verification:
- [ ] `spfs shell --edit <ref>` creates editable runtime
- [ ] File modifications persist within session
- [ ] `spfs commit layer` captures changes
- [ ] New files and deletions handled correctly

---

## Phase 3: Polish and Production (2-4 weeks)

### Overview
Production hardening, monitoring, documentation, and CI/CD setup.

---

### Task 3.1: Monitor Process Port

**Effort**: 3-4 days

Port `spfs-monitor` functionality to macOS:
- Process tree monitoring via kqueue
- Cleanup of orphaned mounts
- Heartbeat mechanism for FUSE

---

### Task 3.2: Mount Cleanup

**Effort**: 2-3 days

Implement automatic cleanup:
- Detect when root_pid exits
- Unregister mount from router
- Clean up scratch directory (Phase 2)

---

### Task 3.3: Performance Optimization

**Effort**: 2-3 days

Optimize hot paths:
- Cache process ancestry lookups (TTL-based)
- Consider DashMap for lock-free route lookups
- Profile and optimize inode operations

---

### Task 3.4: Error Handling and Logging

**Effort**: 1-2 days

Improve operational visibility:
- Structured logging throughout
- Sentry integration for errors
- Helpful error messages for common issues

---

### Task 3.5: Documentation

**Effort**: 2-3 days

Create documentation:
- macOS installation guide
- macFUSE setup instructions
- Troubleshooting guide
- Architecture documentation

---

### Task 3.6: CI/CD Setup

**Effort**: 2-3 days

Set up macOS CI:
- Add macOS runner to GitHub Actions
- Build and test on ARM64 and x86_64
- Integration test job (requires macFUSE)

---

### Phase 3 Success Criteria

#### Automated Verification:
- [ ] macOS CI job passes
- [ ] Performance benchmarks meet targets
- [ ] No memory leaks in long-running tests

#### Manual Verification:
- [ ] Documentation is complete and accurate
- [ ] Error messages are helpful
- [ ] Monitor correctly cleans up orphaned mounts
- [ ] System is stable under load

---

## Risk Register

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| macFUSE kernel extension issues on future macOS | Medium | High | Monitor FSKit development; design for future migration |
| Apple Silicon kernel ext enablement complexity | Low | High | Clear documentation; test on M1/M2/M3 hardware |
| Performance of PID-based routing | Low | Medium | Implement ancestry caching; benchmark early |
| fuser crate macOS bugs | Medium | Medium | Budget time for upstream contributions; have fallback plan |
| Write support complexity | High | Medium | Ship read-only MVP first; iterate on write support |
| macFUSE installation friction | Medium | Medium | Clear documentation; consider FUSE-T for future |

---

## Dependencies

```
Task 1.1 (Structure) ─────┬──► Task 1.2 (Process) ──┬──► Task 1.4 (Mount) ──┬──► Task 1.5 (Router)
                          │                         │                       │
                          └──► Task 1.3 (Handle) ───┘                       └──► Task 1.6 (Module)
                          │
                          └──► Task 1.8 (Backend) ──► Task 1.9 (Env) ──► Task 1.10 (Status)
                                                                                    │
Task 1.7 (CLI) ◄──────────────────────────────────────────────────────────────────┘
        │
        └──► Task 1.11 (Platform Switch) ──► Task 1.12 (Integration)

Phase 2 depends on Phase 1 complete
Phase 3 depends on Phase 2 complete (for full testing)
```

---

## References

- Research: `.llm/shared/research/2025-11-28-spfs-macos-implementation.md`
- Roadmap: `.llm/shared/research/2025-11-29-spfs-macos-tahoe-implementation-roadmap.md`
- gRPC Details: `.llm/shared/research/2025-11-29-spfs-macos-grpc-process-isolation.md`
- WinFSP Context: `.llm/shared/context/2025-11-28-spk-windows-winfsp.md`
- FUSE Context: `.llm/shared/context/2025-11-28-spk-spfs-fuse.md`
- macFUSE: https://osxfuse.github.io/
- fuser crate: https://github.com/cberner/fuser
- libproc crate: https://crates.io/crates/libproc
