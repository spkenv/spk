---
date: 2025-11-29T19:45:00-08:00
researcher: opencode
git_commit: 5c32e2093677ef44b7fc8b227ae20ccec29a1069
branch: main
repository: spk
topic: "OverlayFS Usage in SPFS and UnionFS Alternatives for macOS"
tags: [research, codebase, spfs, overlayfs, unionfs, macos, fuse, filesystem]
status: complete
last_updated: 2025-11-29
last_updated_by: opencode
---

# Research: OverlayFS Usage in SPFS and UnionFS Alternatives for macOS

**Date**: 2025-11-29T19:45:00-08:00  
**Researcher**: opencode  
**Git Commit**: 5c32e2093677ef44b7fc8b227ae20ccec29a1069  
**Branch**: main  
**Repository**: spk

## Research Question

How is overlayfs used in spfs, and can something like unionfs work on macOS as an alternative to the planned macFUSE router approach?

## Summary

OverlayFS is central to SPFS on Linux, providing the layered filesystem at `/spfs` with copy-on-write semantics. The implementation uses either direct Linux syscalls or the `mount` command to create overlay mounts with upper/lower/work directories. **UnionFS-fuse technically works on macOS via macFUSE**, but it does **not** provide the per-process filesystem isolation that is fundamental to how SPFS works. The WinFSP-style router approach (already planned for macOS) remains the recommended solution because it supports per-process views from a single mount point.

**Key Finding**: The router approach is architecturally necessary because SPFS environments need different processes to see different filesystem contents at the same `/spfs` path. Traditional union filesystems (overlayfs, unionfs-fuse) provide a single view to all processes.

## Detailed Findings

### 1. How OverlayFS is Used in SPFS (Linux)

#### Entry Points and Flow

The overlayfs implementation is primarily in these files:

| File | Purpose |
|------|---------|
| `crates/spfs/src/env.rs:513-525` | Primary mount entry point |
| `crates/spfs/src/env.rs:1005-1159` | Mount options and argument construction |
| `crates/spfs/src/env.rs:1217-1574` | Mount via command or syscalls |
| `crates/spfs/src/status_unix.rs:202-270` | Runtime initialization |
| `crates/spfs/src/runtime/storage.rs:131-220` | Directory configuration |
| `crates/spfs/src/runtime/overlayfs.rs` | Kernel feature detection |

#### Directory Structure

From `runtime/storage.rs:185-220`:

```rust
const RUNTIME_DIR: &'static str = "/tmp/spfs-runtime";
const UPPER_DIR: &'static str = "upper";     // Writable layer
const LOWER_DIR: &'static str = "lower";     // Read-only rendered layers
const WORK_DIR: &'static str = "work";       // OverlayFS work directory
```

Default layout:
```
/tmp/spfs-runtime/
├── upper/     # Copy-on-write changes (editable runtimes)
├── lower/     # Base/empty lower dir
└── work/      # OverlayFS internal workdir
```

#### Mount Options

From `env.rs:1008-1073`, SPFS uses these overlayfs options:

| Option | Purpose | Kernel Version |
|--------|---------|----------------|
| `ro` | Read-only mount (non-editable runtimes) | All |
| `index=on` | Maintain hardlink identity | 4.13+ |
| `metacopy=on` | Metadata-only copy-up | 4.19+ |
| `lowerdir+` | Append syntax for lower layers | 6.8+ |

#### Mount Argument Construction

From `env.rs:1105-1159`, the mount arguments are built as:

```
ro,metacopy=on,index=on,lowerdir=/rendered/layer1:/rendered/layer2:/tmp/spfs-runtime/lower,upperdir=/tmp/spfs-runtime/upper,workdir=/tmp/spfs-runtime/work
```

Note: Lower directories are in **reverse order** (rightmost = bottom layer).

#### Two Mount Methods

**1. Via `mount` command** (`env.rs:1217-1243`):
```bash
mount -t overlay -o "$OPTIONS" none /spfs
```

**2. Via direct syscalls** (`env.rs:1246-1574`):
```c
// Syscall sequence:
fd = fsopen("overlay", FSOPEN_CLOEXEC)
fsconfig(fd, FSCONFIG_SET_STRING, "source", "none")
fsconfig(fd, FSCONFIG_SET_STRING, "lowerdir+", "/path/to/layer")
fsconfig(fd, FSCONFIG_SET_STRING, "upperdir", "/upper")
fsconfig(fd, FSCONFIG_SET_STRING, "workdir", "/work")
fsconfig(fd, FSCONFIG_CMD_CREATE)
mount_fd = fsmount(fd, FSMOUNT_CLOEXEC, 0)
mount_setattr(mount_fd, "", AT_EMPTY_PATH, &attrs, size)
move_mount(mount_fd, "", AT_FDCWD, "/spfs", MOVE_MOUNT_F_EMPTY_PATH)
```

The syscall method is preferred (configurable via `config.use_mount_syscalls`).

### 2. Mount Backends in SPFS

From `runtime/storage.rs:238-305`:

```rust
pub enum MountBackend {
    OverlayFsWithRenders,     // Linux default: render layers + overlayfs
    OverlayFsWithFuse,        // FUSE as lowerdir for overlayfs
    FuseOnly,                 // Direct FUSE mount (read-only)
    WinFsp,                   // Windows (default on Windows)
    MacFuse,                  // macOS (planned, default on macOS)
}
```

| Backend | Platform | Isolation Method | Write Support |
|---------|----------|------------------|---------------|
| OverlayFsWithRenders | Linux | Mount namespaces | Yes (upper dir) |
| OverlayFsWithFuse | Linux | Mount namespaces | Yes (upper dir) |
| FuseOnly | Linux | Mount namespaces | No |
| WinFsp | Windows | PID-based router | No (TODO) |
| MacFuse | macOS | PID-based router (planned) | No (planned) |

### 3. Linux Isolation via Mount Namespaces

The key to understanding why overlayfs works on Linux is **mount namespaces**:

From `env.rs:81-84`:
```rust
pub fn enter_mount_namespace(&self) -> Result<ProcessIsInMountNamespace> {
    unshare(CloneFlags::CLONE_NEWNS)?;  // Create new mount namespace
    Ok(ProcessIsInMountNamespace::new())
}
```

Each SPFS runtime creates a **new mount namespace**, so:
- The `/spfs` overlay mount is only visible to that process tree
- Different processes can have completely different `/spfs` contents
- No conflict between concurrent runtimes

**macOS lacks mount namespaces entirely**, which is why a different approach is needed.

### 4. UnionFS-FUSE on macOS

#### Availability

unionfs-fuse ([GitHub](https://github.com/rpodgorny/unionfs-fuse)) **does support macOS** via macFUSE:
- Build with cmake is recommended for macOS
- Has Vagrant-based macOS test infrastructure
- Active maintenance

#### Key Differences from OverlayFS

| Feature | OverlayFS (Linux) | unionfs-fuse |
|---------|-------------------|--------------|
| Implementation | In-kernel | User-space FUSE |
| Performance | Near-native after open | FUSE overhead on all ops |
| Portability | Linux only | Cross-platform |
| Process Isolation | Via namespaces | **None** |

From Linux overlayfs documentation:
> "After a file is opened all operations go directly to the underlying filesystems. This simplifies the implementation and allows native performance."

unionfs-fuse cannot achieve this - all operations pass through FUSE.

#### The Fundamental Problem

unionfs-fuse provides **one filesystem view to all processes**. SPFS requires:
- Process A runs `spfs run pkg1 -- bash` → sees `/spfs` with pkg1 contents
- Process B runs `spfs run pkg2 -- bash` → sees `/spfs` with pkg2 contents
- Both processes run simultaneously on same machine

With unionfs-fuse, you would need:
- Separate mount points (e.g., `/spfs-runtime-1`, `/spfs-runtime-2`)
- Some way to redirect processes to the correct mount
- No longer a unified `/spfs` experience

### 5. The Router Approach (WinFSP / macFUSE)

The Windows WinFSP implementation solves this differently:

From `crates/spfs-vfs/src/winfsp/router.rs:32-38`:
```rust
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,  // PID -> Mount
    default: Arc<Mount>,
}
```

How it works:
1. **Single mount point**: One FUSE mount at `/spfs` (or `C:\spfs`)
2. **Per-request routing**: Every filesystem operation carries caller PID
3. **Process ancestry lookup**: Walk up process tree to find registered runtime
4. **Dynamic registration**: `router.mount(root_pid, env_spec)` registers views

From `router.rs:89-98`:
```rust
fn get_filesystem_for_calling_process(&self) -> Result<Arc<Mount>> {
    let stack = self.get_process_stack()?;  // [child, parent, grandparent, ...]
    let routes = self.routes.read();
    for pid in stack {
        if let Some(mount) = routes.get(&pid) {
            return Ok(Arc::clone(mount));  // First ancestor wins
        }
    }
    Ok(Arc::clone(&self.default))  // Empty mount for unregistered
}
```

### 6. Comparison: UnionFS vs Router for macOS

| Aspect | unionfs-fuse | Router (macFUSE) |
|--------|--------------|------------------|
| **Per-Process Isolation** | No | Yes |
| **Multiple Concurrent Runtimes** | Requires separate mounts | Single mount point |
| **Architecture Match** | Different from Linux/Windows | Matches Windows |
| **Implementation Effort** | Lower (existing project) | Higher (new code) |
| **macFUSE Dependency** | Yes | Yes |
| **Process Tree Tracking** | N/A | Required (via libproc) |
| **gRPC Control Plane** | N/A | Reuse existing vfs.proto |

### 7. Why Router is Recommended for macOS

The existing macOS implementation plan (`.llm/shared/plans/2025-11-29-spfs-macos-implementation.md`) chose the router approach because:

1. **Architectural Consistency**: Same pattern as Windows, easier to maintain
2. **Core Feature Support**: Per-process isolation is fundamental to SPFS
3. **Existing Code Reuse**: 
   - `vfs.proto` gRPC definitions are platform-agnostic
   - Router logic ports from WinFSP with minimal changes
   - Mount/Handle types are similar
4. **Process API Availability**: macOS exposes caller PID via fuser crate's `Request::pid()`

From the plan:
```rust
// macOS process ancestry via libproc
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>> {
    // Use proc_pidinfo() API
}
```

### 8. Could UnionFS-FUSE Be Useful at All?

Potentially, but in a limited way:

**Scenario A: Single-User Development Machine**
If only one runtime runs at a time, unionfs-fuse could work:
- Mount unionfs-fuse at `/spfs`
- Change branches when switching environments
- Simpler than full router implementation

**Scenario B: Hybrid Approach**
Use unionfs-fuse for the overlay mechanics, router for multiplexing:
- Router intercepts all operations
- Delegates to per-runtime unionfs-fuse mounts
- Adds complexity, questionable value

**Scenario C: Testing/Prototyping**
Quick way to validate SPFS concepts on macOS without full implementation.

**Conclusion**: For production SPFS on macOS, the router approach is the right choice.

## Architecture Insights

### Platform Abstraction Pattern

SPFS uses conditional compilation for platform-specific code:

```rust
#[cfg_attr(target_os = "macos", path = "./env_macos.rs")]
#[cfg_attr(windows, path = "./env_win.rs")]
#[cfg_attr(all(unix, not(target_os = "macos")), path = "./env.rs")]
pub mod env;
```

### Isolation Strategies by Platform

```
┌─────────────────────────────────────────────────────────────────────┐
│                    SPFS Isolation Strategies                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Linux:                                                              │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ unshare(CLONE_NEWNS) → mount overlayfs → isolated namespace │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  Windows:                                                            │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ WinFSP mount → Router → PID lookup → per-process Mount      │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  macOS (planned):                                                    │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ macFUSE mount → Router → PID lookup → per-process Mount     │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  macOS (unionfs-fuse alternative - NOT RECOMMENDED):                │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │ unionfs-fuse mount → single view → NO isolation             │    │
│  └─────────────────────────────────────────────────────────────┘    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Code References

### OverlayFS Implementation (Linux)
- `crates/spfs/src/env.rs:513-525` - Main mount entry point
- `crates/spfs/src/env.rs:1005-1073` - OverlayMountOptions struct
- `crates/spfs/src/env.rs:1105-1159` - get_overlay_args()
- `crates/spfs/src/env.rs:1217-1243` - mount_overlayfs_command()
- `crates/spfs/src/env.rs:1246-1574` - mount_overlayfs_syscalls()
- `crates/spfs/src/runtime/storage.rs:131-220` - Directory config
- `crates/spfs/src/runtime/overlayfs.rs:28-88` - Kernel feature detection

### Router Implementation (Windows, template for macOS)
- `crates/spfs-vfs/src/winfsp/router.rs:32-98` - Router struct and routing logic
- `crates/spfs-vfs/src/winfsp/mount.rs:39-152` - Per-runtime Mount
- `crates/spfs-vfs/src/winfsp/handle.rs:12-75` - File handle types
- `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs:57-315` - CLI service/mount

### Platform Abstraction
- `crates/spfs/src/runtime/storage.rs:238-305` - MountBackend enum
- `crates/spfs/src/env_win.rs` - Windows RuntimeConfigurator
- `crates/spfs/src/status_win.rs` - Windows lifecycle

## Historical Context (from .llm/)

### Prior Research
- `.llm/shared/research/2025-11-28-spfs-macos-implementation.md` - Initial macOS feasibility
- `.llm/shared/research/2025-11-29-spfs-macos-grpc-process-isolation.md` - gRPC router details
- `.llm/shared/research/2025-11-29-spfs-macos-tahoe-implementation-roadmap.md` - Implementation roadmap

### Implementation Plan
- `.llm/shared/plans/2025-11-29-spfs-macos-implementation.md` - Detailed 10-16 week plan

### Context Documents
- `.llm/shared/context/2025-11-28-spk-spfs-runtime.md` - SPFS runtime architecture
- `.llm/shared/context/2025-11-28-spk-spfs-fuse.md` - FUSE integration details
- `.llm/shared/context/2025-11-28-spk-windows-winfsp.md` - WinFSP architecture

## Related Research
- `.llm/shared/research/2025-11-28-spfs-macos-implementation.md`
- `.llm/shared/research/2025-11-29-spfs-macos-grpc-process-isolation.md`

## Open Questions

1. **Performance Comparison**: How does the router approach compare to native overlayfs in terms of latency and throughput? Would be useful to benchmark.

2. **Process Ancestry Caching**: The plan mentions caching ancestry lookups. What TTL is optimal to balance performance vs accuracy?

3. **FSKit Future**: Apple's FSKit (macOS 15.4+) could eventually replace macFUSE. Does FSKit expose caller context for routing?

4. **unionfs-fuse Fallback**: Should we support unionfs-fuse as a "simple mode" for single-runtime use cases?

5. **Write Support Priority**: The plan defers write support to Phase 2. Is read-only sufficient for initial adoption?
