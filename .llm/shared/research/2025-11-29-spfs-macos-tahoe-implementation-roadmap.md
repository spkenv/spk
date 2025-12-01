---
date: 2025-11-29T15:00:00-08:00
researcher: opencode
git_commit: 5c32e2093677ef44b7fc8b227ae20ccec29a1069
branch: main
repository: spk
topic: "macOS Tahoe 26 SPFS Implementation Roadmap with macFUSE"
tags: [research, codebase, spfs, macos, fuse, macfuse, tahoe, platform-abstraction, isolation]
status: complete
last_updated: 2025-11-29
last_updated_by: opencode
---

# Research: macOS Tahoe 26 SPFS Implementation Roadmap with macFUSE

**Date**: 2025-11-29T15:00:00-08:00  
**Researcher**: opencode  
**Git Commit**: 5c32e2093677ef44b7fc8b227ae20ccec29a1069  
**Branch**: main  
**Repository**: spk

## Research Question

What would it take to implement macOS support (Tahoe 26) for SPFS utilizing macFUSE?

## Summary

Implementing SPFS on macOS Tahoe (macOS 26) with macFUSE is **architecturally feasible** but requires significant development effort across several areas:

1. **FUSE Layer**: The `fuser` crate already supports macFUSE (untested but actively maintained). The spfs-vfs FUSE implementation is largely platform-agnostic and should work with macFUSE after enabling the `macfuse-4-compat` feature.

2. **Isolation Model**: macOS lacks Linux mount namespaces. The Windows WinFSP implementation provides a proven PID-based routing pattern that can be adapted - a singleton FUSE mount with a router that multiplexes filesystem views based on process ancestry.

3. **Overlay Filesystem**: macOS lacks overlayfs. The `FuseOnly` backend provides read-only support. Write support requires implementing copy-on-write semantics in FUSE userspace.

4. **Privilege Model**: macOS uses entitlements instead of capabilities. macFUSE requires admin installation and, on Apple Silicon, lowered system security (Recovery Mode to enable kernel extensions).

5. **Estimated Effort**: 4-8 weeks for a read-only MVP, additional 4-6 weeks for write support.

## Detailed Findings

### 1. FUSE Implementation Compatibility

#### Current spfs-vfs Architecture

The spfs-vfs crate (`crates/spfs-vfs/src/fuse.rs`) implements a virtual filesystem using the `fuser` crate (v0.15.1). Key structures:

| Component | Location | Description |
|-----------|----------|-------------|
| `Session` | `fuse.rs:691-721` | Wraps `fuser::Session`, implements `fuser::Filesystem` |
| `Filesystem` | `fuse.rs:63-73` | Core filesystem state: inodes, handles, repos |
| `Config` | `fuse.rs:44-60` | Mount configuration (uid, gid, mount options) |
| `Handle` | `fuse.rs:976-1005` | File handle types (BlobFile, BlobStream, Tree) |

#### macFUSE Compatibility Assessment

| Aspect | Status | Notes |
|--------|--------|-------|
| `fuser` crate macOS support | Unofficial/Untested | Compiles for macOS, not CI-tested |
| MacFUSE version | 4.x compatible | v0.15.0 fixed MacFUSE 4.x compatibility |
| Apple Silicon | Requires setup | Kernel extension support must be enabled in Recovery Mode |
| FUSE protocol | Compatible | macFUSE supports standard FUSE protocol |

#### Required Changes

1. **Enable macfuse-4-compat feature**: Add to `Cargo.toml`:
   ```toml
   fuser = { workspace = true, features = ["macfuse-4-compat"] }
   ```

2. **Unmount utility**: Replace `fusermount` with macOS `umount` or `diskutil`:
   - Location: `crates/spfs/src/env.rs:874-910`
   - Current: `fusermount -u` / `fusermount -uz`
   - macOS: `umount <path>` or `diskutil unmount <path>`

3. **Mount option adjustments**: The `nonempty` option (FUSE2) doesn't apply to macFUSE:
   - Location: `crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs:133-136`
   - Change: Remove or conditionally exclude for macOS

4. **macOS-specific FUSE operations** (optional enhancement):
   - `setvolname()`, `getxtimes()`, `exchange()` - macOS-only operations in fuser
   - Location: Would be new additions to `crates/spfs-vfs/src/fuse.rs`

### 2. Process Isolation Strategy

#### The Challenge

macOS lacks Linux mount namespaces (`CLONE_NEWNS`). Each process cannot have its own `/spfs` view through namespace isolation.

#### Proposed Solution: PID-Based Router (WinFSP Pattern)

The Windows WinFSP implementation (`crates/spfs-vfs/src/winfsp/router.rs`) provides a proven pattern:

```
Single macFUSE mount at /spfs
         |
    Router (PID -> Mount mapping)
         |
    +-----------+-----------+
    |           |           |
  Mount A    Mount B     Mount C
  (PID 1234) (PID 5678)  (default)
```

**Key Components to Port**:

| WinFSP Component | macOS Adaptation |
|------------------|------------------|
| `Router::routes` (HashMap<u32, Arc<Mount>>) | Same - HashMap of PID to Mount |
| `get_parent_pids()` via `CreateToolhelp32Snapshot` | Use `proc_pidinfo()` or `sysctl(KERN_PROC)` |
| `FspFileSystemOperationProcessIdF()` | Use `fuse_get_context()->pid` |
| gRPC control plane (tonic) | Reusable - platform agnostic |

**Process Ancestry on macOS**:

```rust
// macOS equivalent of WinFSP's get_parent_pids()
fn get_parent_pids_macos(pid: pid_t) -> Result<Vec<pid_t>> {
    let mut stack = vec![pid];
    let mut current = pid;
    
    loop {
        let mut info: proc_bsdinfo = unsafe { std::mem::zeroed() };
        let ret = unsafe {
            proc_pidinfo(current, PROC_PIDTBSDINFO, 0, 
                         &mut info as *mut _ as *mut c_void,
                         std::mem::size_of::<proc_bsdinfo>() as i32)
        };
        if ret <= 0 { break; }
        
        let parent = info.pbi_ppid as pid_t;
        if parent == 0 || parent == current { break; }
        stack.push(parent);
        current = parent;
    }
    Ok(stack)
}
```

**FUSE Context PID**:

The `fuser` crate provides `Request::uid()` and `Request::pid()` in filesystem callbacks:

```rust
// In fuse.rs lookup/read/etc handlers
fn lookup(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
    let caller_pid = req.pid(); // Available in fuser
    let mount = self.router.get_mount_for_pid(caller_pid)?;
    mount.lookup(parent, name, reply)
}
```

### 3. New Files and Modules Required

#### Crate Structure Changes

```
crates/
  spfs/src/
    env.rs                    # Modify: Add macOS conditionals
    env_macos.rs              # NEW: macOS-specific RuntimeConfigurator
    status_macos.rs           # NEW: macOS runtime lifecycle
    monitor_macos.rs          # NEW: macOS process monitoring
    runtime/
      storage.rs              # Modify: Add MacFuse backend enum variant
  
  spfs-vfs/src/
    fuse.rs                   # Modify: Add router integration
    macos/                    # NEW: macOS-specific implementations
      mod.rs
      router.rs               # PID-based routing (port from winfsp)
      mount.rs                # Per-runtime mount state
      process.rs              # Process ancestry tracking
  
  spfs-cli/
    cmd-fuse-macos/           # NEW: macOS FUSE service CLI
      Cargo.toml
      src/
        cmd_fuse.rs           # Service + mount subcommands (like winfsp)
```

#### MountBackend Enum Extension

```rust
// crates/spfs/src/runtime/storage.rs
pub enum MountBackend {
    #[cfg_attr(all(unix, not(target_os = "macos")), default)]
    OverlayFsWithRenders,
    OverlayFsWithFuse,
    FuseOnly,
    #[cfg_attr(windows, default)]
    WinFsp,
    #[cfg_attr(target_os = "macos", default)]
    MacFuse,  // NEW
}
```

### 4. Overlay Filesystem Alternatives

#### Option A: FuseOnly Backend (Read-Only MVP)

The existing `FuseOnly` backend works without overlayfs:

```
crates/spfs/src/status_unix.rs:182-188:
MountBackend::FuseOnly => {
    with_root.mount_env_fuse(rt).await?;
}
```

**Limitations**:
- Read-only filesystem
- No runtime edits (`spfs shell --edit` unsupported)
- No durable runtime mutations

**Effort**: Minimal - primarily routing integration

#### Option B: FUSE-Based Copy-on-Write (Read-Write)

Implement overlayfs-like semantics in FUSE userspace:

```rust
// Extended Handle enum for write support
enum Handle {
    BlobFile { entry, file: File },           // Read from repo
    BlobStream { entry, stream: ... },        // Remote stream
    Tree { entry },                           // Directory
    // NEW: Write support
    ScratchFile { entry, scratch_path: PathBuf }, // Modified file
    NewFile { path: PathBuf, created: bool },     // Created file
}
```

**Implementation Requirements**:

1. **Scratch directory**: Per-runtime writable area (like overlayfs upperdir)
2. **Write handlers**: Implement `write`, `create`, `setattr`, `unlink` in FUSE
3. **Copy-up semantics**: Copy file from repo to scratch on first write
4. **Whiteout tracking**: Track deletions in memory or marker files

**Effort**: Significant - 4-6 weeks additional

### 5. Privilege and Capability Model

#### macOS vs Linux Privileges

| Linux | macOS Equivalent |
|-------|------------------|
| `CAP_SYS_ADMIN` on binaries | Admin installation of macFUSE |
| `setcap` for capabilities | Codesigning with entitlements |
| Namespace creation | N/A - no equivalent |
| Root for mount operations | `allow_other` requires admin/root |

#### macFUSE Installation Requirements

1. **Intel Macs**: Standard macFUSE installation via DMG or Homebrew
2. **Apple Silicon**: 
   - Boot into Recovery Mode
   - Run `csrutil enable --without kext`
   - Or fully enable third-party kernel extensions

#### Required Entitlements (if distributing app)

```xml
<!-- entitlements.plist -->
<key>com.apple.security.cs.allow-unsigned-executable-memory</key>
<true/>
```

### 6. Runtime Initialization Flow for macOS

#### Proposed Initialization Sequence

```
1. CLI: spfs run <refs> --
   |
2. Build runtime metadata (existing code)
   |
3. Check for running macFUSE service
   |  - If not running: spawn `spfs-fuse-macos service`
   |
4. Send mount request via gRPC
   |  - root_pid: parent shell PID
   |  - env_spec: resolved platform digest
   |
5. Router registers new Mount for PID tree
   |
6. Service confirms mount ready
   |
7. Execute user command with SPFS env vars
   |
8. On exit: unregister mount from router
```

#### Status Module Structure

```rust
// crates/spfs/src/status_macos.rs

pub async fn initialize_runtime(rt: &mut runtime::OwnedRuntime) -> Result<()> {
    // 1. Prepare live layers
    rt.prepare_live_layers().await?;
    
    // 2. Ensure macFUSE service is running
    ensure_fuse_service_running().await?;
    
    // 3. Register this runtime's PID with the router
    let root_pid = std::process::id();
    let env_spec = rt.status.stack.to_env_spec_string();
    
    register_mount(root_pid, &env_spec).await?;
    
    // 4. Save runtime state
    rt.save_state_to_storage().await?;
    
    Ok(())
}

pub async fn exit_runtime(rt: &runtime::OwnedRuntime) -> Result<()> {
    // Deregister mount from router
    let root_pid = rt.status.owner.map(|o| o.pid).unwrap_or_default();
    deregister_mount(root_pid).await?;
    Ok(())
}
```

### 7. Implementation Roadmap

#### Phase 1: Read-Only MVP (4-6 weeks)

| Week | Task | Files Affected |
|------|------|----------------|
| 1-2 | Port PID router from WinFSP | `spfs-vfs/src/macos/router.rs`, `mount.rs` |
| 2-3 | macOS process ancestry tracking | `spfs-vfs/src/macos/process.rs` |
| 3-4 | Integrate router with fuser Filesystem | `spfs-vfs/src/fuse.rs` |
| 4 | Create macOS CLI (service + mount) | `spfs-cli/cmd-fuse-macos/` |
| 5 | macOS status/env modules | `spfs/src/status_macos.rs`, `env_macos.rs` |
| 5-6 | Testing and unmount handling | Integration tests |

#### Phase 2: Write Support (4-6 weeks)

| Week | Task | Files Affected |
|------|------|----------------|
| 7-8 | Scratch directory management | `spfs-vfs/src/macos/scratch.rs` |
| 8-9 | FUSE write operation handlers | `spfs-vfs/src/fuse.rs` |
| 9-10 | Copy-up semantics | `spfs-vfs/src/macos/mount.rs` |
| 10-11 | Whiteout/deletion tracking | `spfs-vfs/src/macos/mount.rs` |
| 11-12 | Edit mode and commit support | `spfs/src/status_macos.rs` |

#### Phase 3: Polish and Production (2-4 weeks)

- CI/CD for macOS builds
- Documentation
- Package distribution (DMG, Homebrew formula)
- Performance optimization

### 8. Risk Assessment

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| macFUSE kernel extension issues on future macOS | Medium | High | Monitor FSKit development (macOS 15+) |
| Apple Silicon compatibility | Low | High | Test on M1/M2/M3 hardware early |
| Performance of PID-based routing | Low | Medium | Cache process ancestry, benchmark |
| fuser crate macOS bugs | Medium | Medium | Be prepared to contribute fixes upstream |
| Write support complexity | High | Medium | Ship read-only MVP first |

## Code References

### Existing Code to Reuse/Adapt

- `crates/spfs-vfs/src/fuse.rs:63-973` - FUSE filesystem implementation
- `crates/spfs-vfs/src/winfsp/router.rs:32-98` - PID routing pattern
- `crates/spfs-vfs/src/winfsp/mount.rs:39-152` - Per-runtime mount state
- `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs:139-315` - Service CLI pattern
- `crates/spfs/src/runtime/storage.rs:238-305` - MountBackend enum

### Linux-Specific Code to Replace

- `crates/spfs/src/env.rs:224-235` - Namespace creation (`unshare(CLONE_NEWNS)`)
- `crates/spfs/src/env.rs:874-910` - `fusermount` unmount calls
- `crates/spfs/src/monitor.rs:370-459` - `/proc` filesystem scanning
- `crates/spfs/src/env.rs:1246-1574` - overlayfs syscalls

## Architecture Insights

### Platform Abstraction Pattern

The codebase uses `cfg_attr` for module path swapping:

```rust
// crates/spfs/src/lib.rs
#[cfg_attr(windows, path = "./env_win.rs")]
#[cfg_attr(target_os = "macos", path = "./env_macos.rs")]  // Add this
pub mod env;
```

This pattern should be extended for macOS-specific modules.

### Service Architecture (Singleton + Router)

Both WinFSP and the proposed macFUSE implementation use a singleton service:

```
┌─────────────────────────────────────────────┐
│            spfs-fuse-macos service          │
│  ┌─────────────────────────────────────┐    │
│  │    macFUSE mount at /spfs           │    │
│  └──────────────┬──────────────────────┘    │
│                 │                            │
│  ┌──────────────▼──────────────────────┐    │
│  │           Router                     │    │
│  │  ┌─────────────────────────────┐    │    │
│  │  │ PID 1234 → Mount A (dev)    │    │    │
│  │  │ PID 5678 → Mount B (prod)   │    │    │
│  │  │ default  → Empty Mount      │    │    │
│  │  └─────────────────────────────┘    │    │
│  └─────────────────────────────────────┘    │
│                                              │
│  ┌─────────────────────────────────────┐    │
│  │    gRPC Service (tonic)             │    │
│  │    - mount(root_pid, env_spec)      │    │
│  │    - unmount(root_pid)              │    │
│  │    - shutdown()                     │    │
│  └─────────────────────────────────────┘    │
└─────────────────────────────────────────────┘
```

## Historical Context

### Prior Research

- `.llm/shared/research/2025-11-28-spfs-macos-implementation.md` - Initial feasibility analysis
- `.llm/shared/context/2025-11-28-spk-windows-winfsp.md` - WinFSP architecture reference
- `.llm/shared/context/2025-11-28-spk-spfs-fuse.md` - FUSE implementation details

### Related Work

- macFUSE: https://osxfuse.github.io/
- fuser crate: https://github.com/cberner/fuser
- FSKit (future): Apple's userspace filesystem API for macOS 15+

## Open Questions

1. **FSKit Timeline**: Should we target FSKit instead of/in addition to macFUSE for macOS 15+? FSKit avoids kernel extensions but currently lacks per-request caller context.

2. **Distribution Model**: How should macFUSE be bundled/required? Options:
   - Require separate macFUSE installation (current approach for OSXFUSE)
   - Bundle macFUSE installer
   - Wait for FSKit adoption

3. **Apple Silicon Testing**: Access to M1/M2/M3 hardware for testing kernel extension enablement.

4. **Write Support Priority**: Is read-only sufficient for initial macOS use cases, or is write support a hard requirement?

5. **Performance Benchmarks**: What are acceptable latency targets for the PID routing lookup on each filesystem operation?
