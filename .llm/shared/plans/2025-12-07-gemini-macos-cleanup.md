# macOS Runtime Cleanup Implementation Plan (Gemini)

## Overview

Fix the macOS runtime cleanup race condition where `spfs-monitor` prematurely deletes runtime metadata because it relies on Linux-specific `/proc` filesystem features. We will solve this by implementing a `sysctl`-based process tracking system in `spfs` (named "Gemini" for the dual-nature of the fix across monitor and fuse) and migrating the macOS monitor to use it.

## Current State Analysis

- **spfs-monitor**: Uses `wait_for_empty_runtime` which on macOS fails to find a mount namespace (`/proc` doesn't exist), logs a warning, and immediately cleans up the runtime.
- **spfs-vfs**: Has a working `ProcessWatcher` utilizing `kqueue` and process ancestry checks using `libproc`, but this logic is private to the VFS crate.
- **Dependency Issue**: `spfs-monitor` (in `spfs`) cannot access the working logic in `spfs-vfs` due to dependency direction.
- **Constraint**: `libproc` has permission issues; we must switch to `sysctl` for process info.

## Desired End State

- **Consistent Monitoring**: On macOS, `spfs-monitor` correctly waits for the root process and its descendants to exit before cleaning up.
- **Shared Infrastructure**: A new `spfs::process` module handles platform-specific process tracking (using `sysctl` on macOS).
- **Reduced Duplication**: `spfs-vfs` reuses the tracking logic from `spfs`, removing the need for `libproc`.
- **Verified**: The `spfs info` "Runtime does not exist" race condition is resolved.

### Key Discoveries:
- `monitor.rs` currently implements the Linux logic by default.
- `lib.rs` uses `#[cfg_attr(target_os = "macos", path = "...")]` to hot-swap modules.
- `sysctl` crate is superior to `libproc` for macOS process inspection.

## What We're NOT Doing

- Changing the Linux monitor behavior.
- Adding Windows monitor implementation (out of scope).
- Refactoring the entire FUSE layer beyond what's needed for process tracking.

## Implementation Approach

1.  **Refactor & Centralize**: Implement `ProcessWatcher` and ancestry logic in `spfs` using `sysctl`.
2.  **Platform Switch**: Create `monitor_macos.rs` and wire it into `lib.rs`.
3.  **Cleanup**: Update `spfs-vfs` to use the new shared implementation.

## Phase 1: Shared Process Infrastructure (spfs crate)

### Overview
Add the necessary dependencies and implement the low-level process tracking primitives in the core crate.

### Changes Required:

#### 1. Add Dependencies
**File**: `crates/spfs/Cargo.toml`
**Changes**: Add `sysctl` and `libc` for macOS target.

```toml
[target.'cfg(target_os = "macos")'.dependencies]
sysctl = "0.7"
libc = { workspace = true }
```

#### 2. Implement Process Tracking
**File**: `crates/spfs/src/process_macos.rs` (New File)
**Changes**: Implement `ProcessWatcher` (using kqueue) and `get_parent_pid` (using sysctl).

```rust
use std::collections::HashSet;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use sysctl::Sysctl; // Use sysctl crate

// Port ProcessWatcher struct from spfs-vfs/src/macos/process.rs
// Implement get_parent_pid using sysctl call "kern.proc.pid"
pub fn get_parent_pid(pid: i32) -> io::Result<i32> {
    // Implementation using sysctl::Ctl::new(&format!("kern.proc.pid.{}", pid))
}

pub fn is_descendant(pid: i32, root: i32) -> bool {
    // Walk up the tree using get_parent_pid
}
```

#### 3. Expose Module
**File**: `crates/spfs/src/lib.rs`
**Changes**: Add process module configuration.

```rust
#[cfg(target_os = "macos")]
#[path = "process_macos.rs"]
pub mod process;
```

### Success Criteria:

#### Automated Verification:
- [x] `spfs` compiles on macOS: `cargo build -p spfs`
- [x] New unit tests in `process_macos.rs` pass: `cargo test -p spfs --lib process`

---

## Phase 2: macOS Monitor Implementation

### Overview
Implement the monitor logic specifically for macOS using the new infrastructure.

### Changes Required:

#### 1. Implement Monitor Logic
**File**: `crates/spfs/src/monitor_macos.rs` (New File)
**Changes**: Implement `wait_for_empty_runtime` matching the signature in `monitor.rs`.

```rust
use crate::runtime;
use crate::process::ProcessWatcher;

pub async fn wait_for_empty_runtime(rt: &runtime::Runtime, _config: &crate::Config) -> crate::Result<()> {
    // 1. Get root PID from runtime status
    // 2. Initialize ProcessWatcher
    // 3. Loop:
    //    - Check if root PID or any descendants are alive
    //    - Wait for kqueue exit event
    //    - Break when no relevant processes remain
}
```

#### 2. Wire Up Monitor
**File**: `crates/spfs/src/lib.rs`
**Changes**: Configure `monitor` module to use macOS implementation.

```rust
#[cfg_attr(windows, path = "./monitor_win.rs")]
#[cfg_attr(target_os = "macos", path = "./monitor_macos.rs")]
pub mod monitor;
```

### Success Criteria:

#### Automated Verification:
- [x] `spfs-monitor` builds: `cargo build --bin spfs-monitor`
- [x] Existing monitor tests pass (where applicable): `cargo test -p spfs --lib monitor`

#### Manual Verification:
- [ ] Verify `spfs info` works consistently inside a runtime:
  ```bash
  spfs shell -
  spfs info && sleep 2 && spfs info
  ```

---

## Phase 3: Refactor FUSE Layer (spfs-vfs)

### Overview
Update the FUSE filesystem to use the shared process tracking, removing technical debt.

### Changes Required:

#### 1. Remove Private Implementation
**File**: `crates/spfs-vfs/src/macos/process.rs`
**Changes**: Delete this file or replace contents with re-exports from `spfs::process`.

#### 2. Update Router
**File**: `crates/spfs-vfs/src/macos/router.rs`
**Changes**: Import `ProcessWatcher` from `spfs::process`.

#### 3. Dependency Cleanup
**File**: `crates/spfs-vfs/Cargo.toml`
**Changes**: Remove `libproc` dependency.

### Success Criteria:

#### Automated Verification:
- [x] `spfs-vfs` builds cleanly: `cargo build -p spfs-vfs`
- [x] Integration tests pass: `make test`

---

## Testing Strategy

### Unit Tests:
- **Ancestry Check**: Verify `is_descendant` correctly identifies child/grandchild processes.
- **Watcher**: Verify `ProcessWatcher` detects exit events for child processes.

### Integration Tests:
- **Lifecycle**: Run the reproduction case (shell + sleep + info) to verify the runtime is preserved.
- **Cleanup**: Verify the runtime IS eventually cleaned up when the shell exits.

### Manual Testing Steps:
1. Start a shell: `spfs run - -- bash`
2. Run `spfs info` immediately.
3. Wait 10 seconds.
4. Run `spfs info` again (Must not fail).
5. Exit the shell.
6. Verify `spfs list` shows the runtime is gone (or marked for cleanup).

## References

- Ticket: `macos-runtime-cleanup-race-condition`
- Research: `.llm/shared/research/2025-12-07-macos-runtime-cleanup-race-condition.md`
