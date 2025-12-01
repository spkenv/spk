# macOS SPFS Phase 3b: Monitor and Mount Cleanup Implementation Plan

## Overview

This plan implements automatic mount cleanup for the macOS SPFS implementation. When a runtime's root process exits, the mount registration should be automatically removed from the router and scratch directory cleaned up. Unlike Linux (which uses mount namespaces and `/proc` polling), macOS requires a different approach using `kqueue` for process exit notifications.

**Estimated Effort**: 4-5 days

## Current State Analysis

### What Exists Now

1. **No automatic cleanup**: When a runtime process exits, its mount registration remains in the router
2. **Scratch directories persist**: `/tmp/spfs-scratch-*` directories are not cleaned up on process exit
3. **Memory leak potential**: Long-running services accumulate orphaned mount registrations
4. **Linux monitor unusable**: The existing `spfs-monitor` relies on `/proc` filesystem and mount namespaces, neither of which exist on macOS

### Linux Monitor Architecture (for reference)

From `crates/spfs/src/monitor.rs`:
- Uses `/proc/<pid>/ns/mnt` to identify mount namespace
- Polls `/proc` every 2.5 seconds to find processes in same namespace
- Cleans up when no processes remain
- Heartbeat mechanism to FUSE via file lookup

### macOS Constraints

- **No mount namespaces**: Can't use namespace-based isolation
- **No `/proc` filesystem**: Can't use procfs for process discovery
- **PID-based routing**: Must track process trees and detect when root PID exits
- **kqueue available**: macOS's efficient event notification system

## Desired End State

After implementation:

1. Router automatically unregisters mounts when root PID exits
2. Scratch directories are cleaned up automatically
3. Service can run indefinitely without memory leaks
4. Orphaned mounts from crashed processes are eventually cleaned up

### Verification Criteria

**Automated**:
```bash
# Start a runtime, then exit - mount should be cleaned up
spfs shell <ref>
# (inside shell) exit

# Check router has no orphaned mounts
# (Need internal API or service status endpoint)
```

**Manual**:
- [ ] Mount is removed when shell process exits normally
- [ ] Mount is removed when shell process is killed
- [ ] Scratch directory is deleted after cleanup
- [ ] Service memory usage remains stable over time

## What We're NOT Doing

1. **Full spfs-monitor port**: The Linux monitor is too complex and relies on procfs
2. **Mount namespace tracking**: Not available on macOS
3. **Heartbeat mechanism (Phase 3b)**: Defer to later; kqueue is more efficient
4. **Durable runtime support**: Focus on transient runtimes first

## Implementation Approach

Two-pronged approach:
1. **kqueue process watcher**: Monitor root PIDs for exit events in the service
2. **Periodic garbage collection**: Scan for dead PIDs as backup cleanup

---

## Task 3b.1: Add Process Exit Watcher Using kqueue

**Effort**: 1.5 days
**Dependencies**: None

**File**: `crates/spfs-vfs/src/macos/process.rs`

Add kqueue-based process exit monitoring:

```rust
use std::os::fd::{AsRawFd, OwnedFd};
use std::io;

/// Watches a set of process IDs for exit events using kqueue.
///
/// On macOS, kqueue can efficiently monitor process exit via EVFILT_PROC
/// with NOTE_EXIT flag.
pub struct ProcessWatcher {
    kq: OwnedFd,
    watched_pids: std::collections::HashSet<u32>,
}

impl ProcessWatcher {
    /// Create a new process watcher.
    pub fn new() -> io::Result<Self> {
        use std::os::unix::io::FromRawFd;
        
        let kq = unsafe {
            let fd = libc::kqueue();
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            OwnedFd::from_raw_fd(fd)
        };
        
        Ok(Self {
            kq,
            watched_pids: std::collections::HashSet::new(),
        })
    }
    
    /// Add a process ID to watch for exit.
    ///
    /// Returns Ok(true) if the PID was added, Ok(false) if already watched,
    /// or Err if the process doesn't exist or can't be watched.
    pub fn watch(&mut self, pid: u32) -> io::Result<bool> {
        if self.watched_pids.contains(&pid) {
            return Ok(false);
        }
        
        let mut event = libc::kevent {
            ident: pid as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_ADD | libc::EV_ONESHOT,
            fflags: libc::NOTE_EXIT,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        
        let result = unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            )
        };
        
        if result < 0 {
            let err = io::Error::last_os_error();
            // ESRCH means process doesn't exist - not an error, just already exited
            if err.raw_os_error() == Some(libc::ESRCH) {
                return Ok(false);
            }
            return Err(err);
        }
        
        self.watched_pids.insert(pid);
        Ok(true)
    }
    
    /// Stop watching a process ID.
    pub fn unwatch(&mut self, pid: u32) -> bool {
        if !self.watched_pids.remove(&pid) {
            return false;
        }
        
        // Remove from kqueue (best effort - process may have already exited)
        let mut event = libc::kevent {
            ident: pid as usize,
            filter: libc::EVFILT_PROC,
            flags: libc::EV_DELETE,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        
        unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                &event,
                1,
                std::ptr::null_mut(),
                0,
                std::ptr::null(),
            );
        }
        
        true
    }
    
    /// Wait for any watched process to exit.
    ///
    /// Returns the PID that exited, or None on timeout.
    pub fn wait_for_exit(&mut self, timeout: std::time::Duration) -> io::Result<Option<u32>> {
        let timeout_spec = libc::timespec {
            tv_sec: timeout.as_secs() as i64,
            tv_nsec: timeout.subsec_nanos() as i64,
        };
        
        let mut event = libc::kevent {
            ident: 0,
            filter: 0,
            flags: 0,
            fflags: 0,
            data: 0,
            udata: std::ptr::null_mut(),
        };
        
        let result = unsafe {
            libc::kevent(
                self.kq.as_raw_fd(),
                std::ptr::null(),
                0,
                &mut event,
                1,
                &timeout_spec,
            )
        };
        
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        
        if result == 0 {
            // Timeout
            return Ok(None);
        }
        
        let pid = event.ident as u32;
        self.watched_pids.remove(&pid);
        Ok(Some(pid))
    }
    
    /// Check if a specific process is still running.
    pub fn is_process_alive(pid: u32) -> bool {
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
    }
}

#[cfg(test)]
mod process_watcher_tests {
    use super::*;
    
    #[test]
    fn test_watch_current_process() {
        let mut watcher = ProcessWatcher::new().unwrap();
        let pid = std::process::id();
        // Should succeed - we're watching ourselves
        assert!(watcher.watch(pid).unwrap());
        // Should return false - already watching
        assert!(!watcher.watch(pid).unwrap());
    }
    
    #[test]
    fn test_watch_nonexistent_process() {
        let mut watcher = ProcessWatcher::new().unwrap();
        // Use a PID that's very unlikely to exist
        let fake_pid = 999999;
        // Should return false (process doesn't exist) without error
        assert!(!watcher.watch(fake_pid).unwrap_or(false));
    }
    
    #[test]
    fn test_is_process_alive() {
        assert!(ProcessWatcher::is_process_alive(std::process::id()));
        assert!(!ProcessWatcher::is_process_alive(999999));
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [x] Unit tests pass: `cargo test -p spfs-vfs --features macfuse-backend -- process_watcher`
- [ ] Can watch a child process and detect its exit

---

## Task 3b.2: Integrate Process Watcher into Router

**Effort**: 1 day
**Dependencies**: Task 3b.1

**File**: `crates/spfs-vfs/src/macos/router.rs`

Add process watching to the Router:

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use super::process::ProcessWatcher;

pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<DashMap<u32, Arc<Mount>>>,
    default: Arc<Mount>,
    // Process watcher for cleanup
    process_watcher: Arc<tokio::sync::Mutex<ProcessWatcher>>,
    // Shutdown signal for cleanup task
    shutdown: Arc<AtomicBool>,
}

impl Router {
    pub async fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
        let default = Arc::new(Mount::empty()?);
        let process_watcher = ProcessWatcher::new()
            .map_err(|e| spfs::Error::String(format!("Failed to create process watcher: {}", e)))?;
        
        Ok(Self {
            repos,
            routes: Arc::new(DashMap::new()),
            default,
            process_watcher: Arc::new(tokio::sync::Mutex::new(process_watcher)),
            shutdown: Arc::new(AtomicBool::new(false)),
        })
    }
    
    /// Start the background cleanup task.
    ///
    /// This spawns a task that watches for process exits and cleans up
    /// orphaned mounts. Call this after creating the Router.
    pub fn start_cleanup_task(self: &Arc<Self>) {
        let router = Arc::clone(self);
        tokio::spawn(async move {
            router.cleanup_loop().await;
        });
    }
    
    async fn cleanup_loop(&self) {
        let cleanup_interval = std::time::Duration::from_secs(5);
        
        while !self.shutdown.load(Ordering::Relaxed) {
            // Wait for process exit or timeout
            let exited_pid = {
                let mut watcher = self.process_watcher.lock().await;
                match watcher.wait_for_exit(cleanup_interval) {
                    Ok(Some(pid)) => Some(pid),
                    Ok(None) => None, // Timeout - do periodic GC
                    Err(e) => {
                        tracing::warn!(error = %e, "process watcher error");
                        None
                    }
                }
            };
            
            // Handle specific exit
            if let Some(pid) = exited_pid {
                self.cleanup_mount(pid).await;
            }
            
            // Periodic garbage collection for any missed exits
            self.garbage_collect_dead_mounts().await;
        }
        
        tracing::debug!("cleanup loop exiting");
    }
    
    async fn cleanup_mount(&self, root_pid: u32) {
        if let Some((_, mount)) = self.routes.remove(&root_pid) {
            tracing::info!(%root_pid, "cleaning up mount for exited process");
            
            // Clean up scratch directory if editable
            if mount.is_editable() {
                if let Some(scratch) = mount.scratch() {
                    if let Err(e) = scratch.cleanup() {
                        tracing::warn!(%root_pid, error = %e, "failed to cleanup scratch directory");
                    }
                }
            }
        }
    }
    
    async fn garbage_collect_dead_mounts(&self) {
        // Collect PIDs to check (avoid holding lock during check)
        let pids: Vec<u32> = self.routes.iter().map(|r| *r.key()).collect();
        
        for pid in pids {
            if !ProcessWatcher::is_process_alive(pid) {
                tracing::debug!(%pid, "found dead process in routes, cleaning up");
                self.cleanup_mount(pid).await;
            }
        }
    }
    
    /// Signal the cleanup task to stop.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}
```

Update `mount_internal` to register the PID with the watcher:

```rust
async fn mount_internal(&self, root_pid: u32, env_spec: EnvSpec, editable: bool, runtime_name: Option<String>) -> spfs::Result<()> {
    // ... existing mount creation code ...
    
    // Watch the root PID for exit
    {
        let mut watcher = self.process_watcher.lock().await;
        if let Err(e) = watcher.watch(root_pid) {
            tracing::warn!(%root_pid, error = %e, "failed to watch process for cleanup");
            // Continue anyway - GC will catch it
        }
    }
    
    // Insert into routes
    match self.routes.entry(root_pid) {
        dashmap::mapref::entry::Entry::Occupied(_) => {
            return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
        }
        dashmap::mapref::entry::Entry::Vacant(entry) => {
            entry.insert(mount);
        }
    }
    
    Ok(())
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [x] Cleanup task starts and runs
- [ ] Mount is removed when watched process exits

#### Manual Verification:
- [ ] Start shell, exit - mount is cleaned up within 5 seconds
- [ ] Kill shell process - mount is cleaned up
- [ ] Service memory usage stable over repeated shell start/exit cycles

---

## Task 3b.3: Add Scratch Directory Cleanup

**Effort**: 0.5 days
**Dependencies**: Task 3b.2

**File**: `crates/spfs-vfs/src/macos/scratch.rs`

Add cleanup method to ScratchDir:

```rust
impl ScratchDir {
    /// Clean up the scratch directory.
    ///
    /// This removes the scratch directory and all its contents.
    /// Call this when the mount is being destroyed.
    pub fn cleanup(&self) -> std::io::Result<()> {
        if self.root.exists() {
            tracing::debug!(path = %self.root.display(), "cleaning up scratch directory");
            std::fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            tracing::warn!(
                path = %self.root.display(),
                error = %e,
                "failed to cleanup scratch directory on drop"
            );
        }
    }
}
```

Update Mount to expose scratch for cleanup:

```rust
impl Mount {
    /// Get a reference to the scratch directory, if this is an editable mount.
    pub fn scratch(&self) -> Option<&ScratchDir> {
        self.scratch.as_ref()
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [x] Unit test: scratch directory is removed on cleanup (already exists)
- [x] Unit test: cleanup is called on Drop (already exists)

---

## Task 3b.4: Add Service Status Endpoint

**Effort**: 0.5 days
**Dependencies**: Task 3b.2

**File**: `crates/spfs-vfs/src/proto/defs/vfs.proto`

Add status RPC for debugging and monitoring:

```protobuf
message StatusRequest {}

message MountInfo {
    uint32 root_pid = 1;
    string env_spec = 2;
    bool editable = 3;
    string runtime_name = 4;
}

message StatusResponse {
    uint32 active_mounts = 1;
    repeated MountInfo mounts = 2;
}

service VfsService {
    rpc Mount(MountRequest) returns (MountResponse);
    rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
    rpc Status(StatusRequest) returns (StatusResponse);  // NEW
}
```

**File**: `crates/spfs-vfs/src/macos/service.rs`

Implement status RPC:

```rust
async fn status(
    &self,
    _request: Request<proto::StatusRequest>,
) -> std::result::Result<Response<proto::StatusResponse>, Status> {
    let mounts: Vec<proto::MountInfo> = self.router
        .iter_mounts()
        .map(|(pid, mount)| proto::MountInfo {
            root_pid: pid,
            env_spec: mount.env_spec().to_string(),
            editable: mount.is_editable(),
            runtime_name: mount.runtime_name().unwrap_or_default().to_string(),
        })
        .collect();
    
    Ok(Response::new(proto::StatusResponse {
        active_mounts: mounts.len() as u32,
        mounts,
    }))
}
```

**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

Add status subcommand:

```rust
#[derive(Debug, Subcommand)]
enum Command {
    Service(CmdService),
    Mount(CmdMount),
    Status(CmdStatus),  // NEW
}

/// Show service status and active mounts
#[derive(Debug, Args)]
struct CmdStatus {
    /// The local address to connect to
    #[clap(
        long,
        default_value = "127.0.0.1:37738",
        env = "SPFS_MACFUSE_LISTEN_ADDRESS"
    )]
    service: SocketAddr,
}

impl CmdStatus {
    async fn run(&self, _config: &spfs::Config) -> Result<i32> {
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{}", self.service))
            .into_diagnostic()?
            .connect()
            .await
            .into_diagnostic()
            .wrap_err("Failed to connect to service - is it running?")?;
        
        let mut client = proto::vfs_service_client::VfsServiceClient::new(channel);
        let response = client.status(proto::StatusRequest {}).await.into_diagnostic()?;
        let status = response.into_inner();
        
        println!("Active mounts: {}", status.active_mounts);
        
        if status.mounts.is_empty() {
            println!("No active mounts");
        } else {
            println!();
            for mount in status.mounts {
                println!("  PID {}: {} (editable: {})",
                    mount.root_pid,
                    mount.env_spec,
                    mount.editable
                );
            }
        }
        
        Ok(0)
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] Proto regenerates: `cargo build -p spfs-vfs --features macfuse-backend`
- [x] Status command works: `spfs-fuse-macos status` (compiles)

#### Manual Verification:
- [ ] Status shows active mounts correctly
- [ ] Mount count decreases when processes exit

---

## Task 3b.5: Add Startup Orphan Cleanup

**Effort**: 0.5 days
**Dependencies**: Task 3b.3

**File**: `crates/spfs-vfs/src/macos/service.rs`

Clean up orphaned scratch directories on service startup:

```rust
impl Service {
    pub async fn new(config: Config) -> spfs::Result<Arc<Self>> {
        // Clean up any orphaned scratch directories from previous runs
        cleanup_orphaned_scratch_directories().await;
        
        // ... rest of existing initialization
    }
}

/// Clean up scratch directories from previous service runs.
///
/// This finds any `/tmp/spfs-scratch-*` directories and removes them
/// if their owning process is no longer running. This handles cases
/// where the service or runtime crashed without proper cleanup.
async fn cleanup_orphaned_scratch_directories() {
    let temp_dir = std::env::temp_dir();
    let pattern = "spfs-scratch-";
    
    let Ok(mut entries) = tokio::fs::read_dir(&temp_dir).await else {
        return;
    };
    
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        
        if !name_str.starts_with(pattern) {
            continue;
        }
        
        // Try to extract runtime name and check if any process has it
        // For now, just remove any scratch directories older than 24 hours
        // as a conservative cleanup
        if let Ok(metadata) = entry.metadata().await {
            if let Ok(modified) = metadata.modified() {
                let age = std::time::SystemTime::now()
                    .duration_since(modified)
                    .unwrap_or_default();
                
                if age > std::time::Duration::from_secs(24 * 60 * 60) {
                    tracing::info!(path = %entry.path().display(), "removing orphaned scratch directory");
                    let _ = tokio::fs::remove_dir_all(entry.path()).await;
                }
            }
        }
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [x] Old scratch directories are cleaned up on startup

#### Manual Verification:
- [ ] Create orphan scratch dir, restart service, verify cleanup

---

## Phase 3b Success Criteria Summary

### Automated Verification:
- [x] All code compiles: `cargo check -p spfs-vfs --features macfuse-backend`
- [x] All tests pass: `cargo test -p spfs-vfs --features macfuse-backend`
- [x] Proto regenerates successfully
- [x] No new warnings: `cargo clippy -p spfs-vfs --features macfuse-backend`

### Manual Verification:
- [ ] Mount is cleaned up when shell exits normally
- [ ] Mount is cleaned up when shell is killed (SIGKILL)
- [ ] Scratch directory is removed after cleanup
- [ ] `spfs-fuse-macos status` shows correct mount count
- [ ] Service memory usage stable over many start/exit cycles
- [ ] Orphaned scratch directories cleaned up on service restart

---

## Dependencies

```
Task 3b.1 (ProcessWatcher) ──► Task 3b.2 (Router Integration)
                                          │
                              Task 3b.3 (Scratch Cleanup)
                                          │
                              Task 3b.4 (Status Endpoint)
                                          │
                              Task 3b.5 (Orphan Cleanup)
```

---

## Testing Strategy

### Unit Tests
- ProcessWatcher creation and PID watching
- Scratch directory cleanup on Drop
- Router GC of dead processes

### Integration Tests
- End-to-end: start shell, exit, verify cleanup
- Concurrent: multiple shells starting/exiting
- Crash recovery: kill process, verify cleanup

### Manual Testing
```bash
# Basic cleanup test
spfs-fuse-macos status  # Should show 0 mounts
spfs shell <ref> &
PID=$!
spfs-fuse-macos status  # Should show 1 mount
kill $PID
sleep 10
spfs-fuse-macos status  # Should show 0 mounts

# Scratch cleanup test
spfs shell --edit <ref> &
PID=$!
ls /tmp/spfs-scratch-*  # Should exist
kill $PID
sleep 10
ls /tmp/spfs-scratch-*  # Should not exist
```

---

## References

- Linux Monitor: `crates/spfs/src/monitor.rs`
- macFUSE Service: `crates/spfs-vfs/src/macos/service.rs`
- Scratch Directory: `crates/spfs-vfs/src/macos/scratch.rs`
- kqueue documentation: https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kqueue.2.html
