# macOS SPFS Architecture

This document explains how SPFS works on macOS using macFUSE, and how it differs from the Linux and Windows implementations.

## Overview

SPFS (Spk File System) provides a virtual filesystem that presents package contents at `/spfs`. Each platform uses a different approach to achieve this:

| Platform | Backend | Isolation Mechanism | Write Support |
|----------|---------|---------------------|---------------|
| Linux | FUSE + OverlayFS | Mount namespaces | OverlayFS upper dir |
| Windows | WinFSP | PID-based router | Copy-on-write scratch |
| macOS | macFUSE | PID-based router | Copy-on-write scratch (`FuseWithScratch`) |

## Architecture Comparison

### Linux: Mount Namespace Isolation

On Linux, SPFS uses **mount namespaces** to provide isolation between different runtime environments.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Linux Host                                │
│  ┌─────────────────────┐    ┌─────────────────────┐             │
│  │   Namespace A       │    │   Namespace B       │             │
│  │ ┌─────────────────┐ │    │ ┌─────────────────┐ │             │
│  │ │ /spfs (overlayfs)│ │    │ │ /spfs (overlayfs)│ │             │
│  │ │  - lower: FUSE   │ │    │ │  - lower: FUSE   │ │             │
│  │ │  - upper: tmpfs  │ │    │ │  - upper: tmpfs  │ │             │
│  │ └─────────────────┘ │    │ └─────────────────┘ │             │
│  │ Process Tree A      │    │ Process Tree B      │             │
│  └─────────────────────┘    └─────────────────────┘             │
└─────────────────────────────────────────────────────────────────┘
```

**Key characteristics:**
- Each runtime gets its own mount namespace
- OverlayFS combines a FUSE lower layer with a writable upper layer
- Kernel-level isolation - processes cannot see each other's mounts
- `unshare(CLONE_NEWNS)` creates new namespaces
- Most performant solution due to kernel-level implementation

**Code location:** `crates/spfs-vfs/src/fuse.rs`, `crates/spfs/src/env.rs`

### Windows: WinFSP with PID-based Routing

Windows lacks mount namespaces, so SPFS uses a **singleton router** pattern with WinFSP.

```
┌─────────────────────────────────────────────────────────────────┐
│                      Windows Host                                │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                spfs-winfsp service                       │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │         WinFSP mount at S:\ (or /spfs)          │    │    │
│  │  └──────────────────────┬──────────────────────────┘    │    │
│  │                         │                               │    │
│  │  ┌──────────────────────▼──────────────────────────┐    │    │
│  │  │                    Router                        │    │    │
│  │  │  ┌────────────────────────────────────────┐     │    │    │
│  │  │  │ routes: HashMap<PID, Arc<Mount>>       │     │    │    │
│  │  │  │   PID 1234 → Mount A (env: dev/base)   │     │    │    │
│  │  │  │   PID 5678 → Mount B (env: prod/tools) │     │    │    │
│  │  │  │   default  → Empty Mount                │     │    │    │
│  │  │  └────────────────────────────────────────┘     │    │    │
│  │  └──────────────────────────────────────────────────┘    │    │
│  │                                                          │    │
│  │  ┌──────────────────────────────────────────────────┐    │    │
│  │  │           gRPC Service (tonic)                   │    │    │
│  │  │    - mount(root_pid, env_spec)                   │    │    │
│  │  │    - shutdown()                                  │    │    │
│  │  │    Listen: 127.0.0.1:37737                       │    │    │
│  │  └──────────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌─────────────────────┐    ┌─────────────────────┐             │
│  │ Process Tree A      │    │ Process Tree B      │             │
│  │ (root PID 1234)     │    │ (root PID 5678)     │             │
│  │ sees Mount A view   │    │ sees Mount B view   │             │
│  └─────────────────────┘    └─────────────────────┘             │
└─────────────────────────────────────────────────────────────────┘
```

**Key characteristics:**
- Single WinFSP mount serves all processes
- Router intercepts each FS request and checks caller PID
- Process ancestry chain walked to find registered root PID
- Different process trees see different filesystem views
- gRPC service for runtime registration

**Code location:** `crates/spfs-vfs/src/winfsp/`

### macOS: macFUSE with PID-based Routing

macOS also lacks mount namespaces, so it uses the same **PID-based router** pattern as Windows, but with macFUSE (via the `fuser` crate).

```
┌─────────────────────────────────────────────────────────────────┐
│                       macOS Host                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │              spfs-fuse-macos service                     │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │       macFUSE mount at /spfs                     │    │    │
│  │  │       (fuser::Session with Router)               │    │    │
│  │  └──────────────────────┬──────────────────────────┘    │    │
│  │                         │                               │    │
│  │  ┌──────────────────────▼──────────────────────────┐    │    │
│  │  │                    Router                        │    │    │
│  │  │  ┌────────────────────────────────────────┐     │    │    │
│  │  │  │ routes: HashMap<PID, Arc<Mount>>       │     │    │    │
│  │  │  │   PID 1234 → Mount A (env: dev/base)   │     │    │    │
│  │  │  │   PID 5678 → Mount B (env: prod/tools) │     │    │    │
│  │  │  │   default  → Empty Mount                │     │    │    │
│  │  │  └────────────────────────────────────────┘     │    │    │
│  │  └──────────────────────────────────────────────────┘    │    │
│  │                                                          │    │
│  │  ┌──────────────────────────────────────────────────┐    │    │
│  │  │           gRPC Service (tonic)                   │    │    │
│  │  │    - mount(root_pid, env_spec)                   │    │    │
│  │  │    - shutdown()                                  │    │    │
│  │  │    Listen: 127.0.0.1:37738                       │    │    │
│  │  └──────────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                  │
│  ┌─────────────────────┐    ┌─────────────────────┐             │
│  │ Process Tree A      │    │ Process Tree B      │             │
│  │ (root PID 1234)     │    │ (root PID 5678)     │             │
│  │ sees Mount A view   │    │ sees Mount B view   │             │
│  └─────────────────────┘    └─────────────────────┘             │
└─────────────────────────────────────────────────────────────────┘
```

**Key characteristics:**
- Single macFUSE mount serves all processes
- Router intercepts each FS request and checks caller PID via `libproc`
- Process ancestry chain walked to find registered root PID
- Different process trees see different filesystem views
- gRPC service for runtime registration (port 37738, different from Windows)
- Uses `fuser` crate with `macfuse-4-compat` feature

**Code location:** `crates/spfs-vfs/src/macos/`

## macOS Implementation Details

### Component Structure

```
crates/spfs-vfs/src/macos/
├── mod.rs        # Module exports, Config, Service
├── config.rs     # Configuration for macFUSE service
├── service.rs    # Service lifecycle and gRPC implementation
├── router.rs     # PID-based routing, implements fuser::Filesystem
├── mount.rs      # Per-runtime filesystem, inode management
├── handle.rs     # File handle types (BlobFile, BlobStream, Tree, ScratchFile)
├── scratch.rs    # Scratch directory for copy-on-write editable mounts
└── process.rs    # Process ancestry tracking via libproc
```

### Request Flow

When an application makes a filesystem call (e.g., `stat("/spfs/foo/bar")`):

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

### Process Ancestry Tracking

macOS uses the `libproc` crate to walk the process tree:

```rust
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>, ProcessError> {
    let mut current = root.unwrap_or(std::process::id() as i32);
    let mut stack = vec![current];
    
    for _ in 0..MAX_DEPTH {
        let info: BSDInfo = pidinfo(current, 0)?;
        let parent = info.pbi_ppid as i32;
        
        if parent == 0 || parent == current || current == 1 {
            break;
        }
        
        stack.push(parent);
        current = parent;
    }
    
    Ok(stack)
}
```

This is equivalent to the Windows implementation which uses `NtQueryInformationProcess`.

### Mount and Inode Management

Each `Mount` instance manages:
- **Inode table** (`DashMap<u64, Arc<Entry<u64>>>`) - Maps inode numbers to manifest entries
- **Handle table** (`DashMap<u64, Handle>`) - Tracks open files/directories
- **Repositories** - List of repos to search for blob data

The manifest is pre-processed when a mount is created:
1. Walk the manifest tree
2. Allocate inode numbers (starting at 1 for root)
3. Store entries in the inode table

### Editable Mounts (FuseWithScratch)

macOS supports editable runtimes through the `FuseWithScratch` mount backend. This enables copy-on-write semantics using a scratch directory.

#### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Editable Mount Flow                          │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                     Router                               │    │
│  │  Delegates write operations to Mount when editable=true  │    │
│  └──────────────────────┬──────────────────────────────────┘    │
│                         │                                        │
│                         ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                      Mount                               │    │
│  │  ┌─────────────────┐    ┌─────────────────────────┐     │    │
│  │  │ Read path:      │    │ Write path:             │     │    │
│  │  │ - Check scratch │    │ - create() → scratch    │     │    │
│  │  │ - Fall back to  │    │ - write() → scratch     │     │    │
│  │  │   repository    │    │ - unlink() → whiteout   │     │    │
│  │  └─────────────────┘    │ - mkdir() → scratch     │     │    │
│  │                         │ - rmdir() → whiteout    │     │    │
│  │                         │ - rename() → scratch    │     │    │
│  │                         └─────────────────────────┘     │    │
│  └──────────────────────┬──────────────────────────────────┘    │
│                         │                                        │
│                         ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │                   ScratchDir                             │    │
│  │  Location: /tmp/spfs-scratch-{runtime_name}/             │    │
│  │                                                          │    │
│  │  ┌─────────────────────────────────────────────────┐    │    │
│  │  │ /tmp/spfs-scratch-abc123/                       │    │    │
│  │  │ ├── bin/                                        │    │    │
│  │  │ │   └── my-script        (new file)             │    │    │
│  │  │ ├── lib/                                        │    │    │
│  │  │ │   └── .wh.deleted.so   (whiteout marker)      │    │    │
│  │  │ └── src/                                        │    │    │
│  │  │     └── main.rs          (modified file)        │    │    │
│  │  └─────────────────────────────────────────────────┘    │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

#### ScratchDir Operations

The `ScratchDir` struct manages the scratch directory:

```rust
impl ScratchDir {
    /// Create a new scratch directory for a runtime
    pub fn new(runtime_name: &str) -> Result<Self>;
    
    /// Get the path for a file in scratch
    pub fn path_for(&self, relative_path: &str) -> PathBuf;
    
    /// Check if a file exists in scratch
    pub fn exists(&self, relative_path: &str) -> bool;
    
    /// Check if a file has been deleted (whiteout marker)
    pub fn is_deleted(&self, relative_path: &str) -> bool;
    
    /// Mark a file as deleted (create whiteout)
    pub fn mark_deleted(&self, relative_path: &str) -> Result<()>;
    
    /// List changes for commit
    pub fn list_changes(&self) -> Result<Vec<Change>>;
}
```

#### Whiteout Files

Deleted files are tracked using whiteout markers (similar to OverlayFS):
- When a file is deleted via `unlink()`, a `.wh.{filename}` marker is created
- On `lookup()`, if a whiteout exists, return `ENOENT`
- On commit, whiteouts indicate files to remove from the manifest

#### Handle Types

The `Handle` enum includes a `ScratchFile` variant for writable files:

```rust
pub enum Handle {
    BlobFile { ... },      // Read-only file from repository
    BlobStream { ... },    // Streaming read from repository
    Tree { ... },          // Directory listing
    ScratchFile {          // Writable file in scratch
        file: std::fs::File,
        path: PathBuf,
    },
}
```

### gRPC Protocol

The macOS service reuses the same protobuf definition as Windows (`vfs.proto`):

```protobuf
service VfsService {
    rpc Mount(MountRequest) returns (MountResponse);
    rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
}

message MountRequest {
    uint32 root_pid = 1;
    string env_spec = 2;
    bool editable = 3;       // Enable write support with scratch directory
    string runtime_name = 4; // Used for scratch directory naming
}
```

## Key Differences from Windows

| Aspect | Windows (WinFSP) | macOS (macFUSE) |
|--------|------------------|-----------------|
| FUSE library | `winfsp` crate | `fuser` crate |
| Process info API | `NtQueryInformationProcess` | `libproc::pidinfo` |
| gRPC port | 37737 | 37738 |
| File context mode | Descriptor mode | Direct callbacks |
| Mount syntax | Drive letter (S:\) or /spfs | /spfs only |
| Kernel extension | WinFSP driver | macFUSE kext |

## Key Differences from Linux

| Aspect | Linux | macOS |
|--------|-------|-------|
| Isolation | Mount namespaces | PID-based router |
| OverlayFS | Native kernel support | Not available |
| Write support | OverlayFS upper dir | Copy-on-write scratch (`FuseWithScratch`) |
| FUSE library | `fuser` crate | `fuser` + `macfuse-4-compat` |
| Process per runtime | Each gets own namespace | Shared FUSE mount |
| Performance | Faster (kernel isolation) | Slight overhead (PID lookup) |

## Usage

### Starting the Service

```bash
# Start the macFUSE service daemon
spfs-fuse-macos service /spfs

# Or with custom listen address
spfs-fuse-macos service --listen 127.0.0.1:9999 /spfs
```

### Mounting an Environment

```bash
# Mount for the current shell's process tree (read-only)
spfs-fuse-macos mount my-package/1.0.0

# Mount for a specific root PID
spfs-fuse-macos mount --root-process 12345 my-package/1.0.0

# Mount with write support (editable)
spfs-fuse-macos mount --editable my-package/1.0.0

# Editable mount with custom runtime name
spfs-fuse-macos mount --editable --runtime my-dev-session my-package/1.0.0
```

### Editable Workflow

```bash
# 1. Start an editable shell session
spfs shell --edit my-package/1.0.0

# 2. Make changes to files in /spfs
echo "#!/bin/bash" > /spfs/bin/my-script
chmod +x /spfs/bin/my-script

# 3. Commit changes back to the repository
spfs commit --message "Add my-script"
```

### Stopping the Service

```bash
spfs-fuse-macos service --stop
```

## Prerequisites

1. **macFUSE** must be installed:
   ```bash
   brew install --cask macfuse
   ```

2. On Apple Silicon, you may need to enable kernel extensions in Recovery Mode.

3. The `/spfs` mount point must exist and be accessible.

## Current Status

### Implemented (Phase 1) - Read-Only Support
- [x] Project structure and Cargo configuration
- [x] Process ancestry tracking via libproc
- [x] Handle types (BlobFile, BlobStream, Tree)
- [x] Mount implementation with inode management
- [x] Router with PID-based routing
- [x] Service with gRPC control plane
- [x] CLI binary (spfs-fuse-macos)
- [x] Platform module switching (env_macos.rs, status_macos.rs)

### Implemented (Phase 2) - Write Support
- [x] `MountBackend::FuseWithScratch` variant for editable runtimes
- [x] Scratch directory management (`ScratchDir` in `scratch.rs`)
- [x] Write operations: `write()`, `create()`, `unlink()`, `mkdir()`, `rmdir()`, `rename()`, `setattr()`
- [x] `ScratchFile` handle type for writable files
- [x] gRPC protocol updated with `editable` flag
- [x] CLI `--editable` flag for `spfs-fuse-macos mount`
- [x] Commit support reads from scratch directory
- [x] Integration tests (24 tests passing)

### Planned (Phase 3)
- [ ] Monitor process for orphaned mount cleanup
- [ ] Performance optimizations (ancestry caching)
- [ ] Copy-up on open for existing files (see Known Limitations)
- [ ] macOS CI/CD integration

## Known Limitations

### Copy-up for Existing Files

Currently, writing to an existing file from the base layer (repository) requires explicit copy-up. The current behavior:

| Operation | On New File | On Scratch File | On Repository File |
|-----------|-------------|-----------------|---------------------|
| `create()` | Creates in scratch | N/A | Creates new in scratch |
| `write()` | Works | Works | Returns `EROFS` |
| `open(O_WRONLY)` | Works | Works | Returns `EROFS` |

**Workaround**: To modify an existing file, first copy it to scratch:
```bash
# Copy the file to enable writes
cp /spfs/lib/config.json /tmp/config.json.tmp
rm /spfs/lib/config.json
cp /tmp/config.json.tmp /spfs/lib/config.json
# Now you can edit it
```

**Future improvement**: Implement automatic copy-up in `open()` when write flags (`O_WRONLY`, `O_RDWR`) are detected. This would transparently copy the file to scratch before allowing writes.

### Scratch Directory Persistence

The scratch directory (`/tmp/spfs-scratch-{runtime}/`) is not automatically cleaned up. If a runtime crashes or is terminated abnormally, the scratch directory may remain. This is tracked for Phase 3 implementation.

## Troubleshooting

### "Operation not permitted" on /spfs
- Ensure macFUSE is installed and kernel extension is loaded
- Check if the mount point exists: `sudo mkdir -p /spfs`
- Verify the service is running: `pgrep -f spfs-fuse-macos`

### "Service is not running"
- Start the service: `spfs-fuse-macos service /spfs`
- Check logs for startup errors

### Process sees empty /spfs
- The process may not be a descendant of a registered root PID
- Use `--root-process` to explicitly set the root PID
- Verify mount was registered: check service logs

## References

- [macFUSE](https://osxfuse.github.io/)
- [fuser crate](https://github.com/cberner/fuser)
- [libproc crate](https://crates.io/crates/libproc)
- [Implementation Plan](.llm/shared/plans/2025-11-29-spfs-macos-implementation.md)
