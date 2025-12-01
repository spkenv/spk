---
date: 2025-11-29T18:30:00-08:00
researcher: opencode
git_commit: 5c32e2093677ef44b7fc8b227ae20ccec29a1069
branch: main
repository: spk
topic: "macOS Process Isolation via gRPC with macFUSE - WinFSP Pattern Adaptation"
tags: [research, codebase, spfs, macos, fuse, macfuse, grpc, process-isolation, router]
status: complete
last_updated: 2025-11-29
last_updated_by: opencode
---

# Research: macOS Process Isolation via gRPC with macFUSE

**Date**: 2025-11-29T18:30:00-08:00  
**Researcher**: opencode  
**Git Commit**: 5c32e2093677ef44b7fc8b227ae20ccec29a1069  
**Branch**: main  
**Repository**: spk

## Research Question

How would we implement WinFSP-style process isolation using a gRPC server with macFUSE on macOS? What are the key components needed to enable per-process filesystem views without Linux mount namespaces?

## Summary

The WinFSP implementation provides a proven architecture for process-isolated virtual filesystems that can be adapted for macOS with macFUSE. The key components are:

1. **Router**: A singleton middleware that intercepts all FUSE operations and routes them to per-runtime `Mount` instances based on the calling process's ancestry chain.

2. **gRPC Control Plane**: A tonic-based service that accepts `mount(root_pid, env_spec)` requests from clients, allowing external processes to register filesystem views for specific process trees.

3. **Process Ancestry Tracking**: On macOS, use the `libproc` crate or raw `proc_pidinfo()` API to walk the process tree from any PID up to launchd (PID 1).

4. **FUSE Context PID**: The fuser crate's `Request::pid()` method provides the caller's PID on every filesystem operation, enabling the router to identify which mount to use.

5. **Handle Types**: Support for both seekable local files (`BlobFile`) and sequential remote streams (`BlobStream`) per the existing WinFSP pattern.

The implementation requires creating a new `crates/spfs-vfs/src/macos/` module with `router.rs`, `mount.rs`, and `process.rs`, plus a CLI binary at `crates/spfs-cli/cmd-fuse-macos/`.

## Detailed Findings

### 1. WinFSP Router Architecture

The WinFSP router (`crates/spfs-vfs/src/winfsp/router.rs`) demonstrates the core pattern for process isolation:

#### Router Struct (`router.rs:32-38`)
```rust
#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,  // PID -> Mount mapping
    default: Arc<Mount>,  // Empty mount for unregistered processes
}
```

**Key Design Decisions:**
- `Clone` trait allows same instance to be shared between WinFSP filesystem and gRPC service
- `RwLock` enables concurrent reads during filesystem operations with occasional writes during mount registration
- Default mount with empty manifest provides safe fallback for processes not in any runtime

#### Mount Registration (`router.rs:55-76`)
```rust
pub async fn mount(&self, root_pid: u32, env_spec: EnvSpec) -> spfs::Result<()> {
    // Compute manifest from repositories
    let manifest = spfs::compute_environment_manifest(&env_spec, repo).await?;
    
    // Create new Mount with pre-allocated inodes
    let mount = Mount::new(rt, self.repos.clone(), manifest)?;
    
    // Register in routes map
    let mut routes = self.routes.write().expect("lock is never poisoned");
    routes.insert(root_pid, Arc::new(mount));
    Ok(())
}
```

#### Operation Routing (`router.rs:89-98`)
```rust
fn get_filesystem_for_calling_process(&self) -> Result<Arc<Mount>> {
    let stack = self.get_process_stack()?;  // [child, parent, grandparent, ...]
    let routes = self.routes.read().expect("Lock is never poisoned");
    for pid in stack {
        if let Some(mount) = routes.get(&pid).map(Arc::clone) {
            return Ok(mount);  // First ancestor with registered mount wins
        }
    }
    Ok(Arc::clone(&self.default))  // Fallback to empty mount
}
```

**macOS Adaptation**: The fuser crate provides `Request::pid()` in every callback, which can be used identically to WinFSP's `FspFileSystemOperationProcessIdF()`.

### 2. gRPC Service Architecture

The gRPC control plane uses tonic with a simple proto definition (`crates/spfs-vfs/src/proto/defs/vfs.proto`):

```protobuf
message MountRequest {
    uint32 root_pid = 1;
    string env_spec = 2;
}
message MountResponse {}

message ShutdownRequest {}
message ShutdownResponse {}

service VfsService {
    rpc Mount(MountRequest) returns (MountResponse);
    rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
}
```

#### Service Implementation (`winfsp/mod.rs:193-210`)
```rust
#[async_trait]
impl VfsService for Arc<Service> {
    async fn mount(&self, request: Request<proto::MountRequest>) 
        -> std::result::Result<Response<proto::MountResponse>, Status> 
    {
        let inner = request.into_inner();
        let env_spec = spfs::tracking::EnvSpec::parse(&inner.env_spec)?;
        self.router.mount(inner.root_pid, env_spec).await?;
        Ok(Response::new(proto::MountResponse {}))
    }
}
```

**macOS Implementation Notes:**
- The proto definitions are platform-agnostic and can be reused
- tonic server/client code is platform-agnostic
- Default listen address pattern (`127.0.0.1:37737`) can be adapted for macOS

### 3. Process Ancestry on macOS

The Windows implementation uses `CreateToolhelp32Snapshot` (`router.rs:548-584`). On macOS, we need an equivalent:

#### Using `libproc` Crate (Recommended)
```rust
use libproc::proc_pid::pidinfo;
use libproc::bsd_info::BSDInfo;

/// macOS equivalent of WinFSP's get_parent_pids()
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>> {
    let mut current = match root {
        Some(pid) => pid,
        None => std::process::id() as i32,
    };
    
    let mut stack = vec![current];
    const MAX_DEPTH: usize = 100;
    
    for _ in 0..MAX_DEPTH {
        let info: BSDInfo = pidinfo(current, 0)?;
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
```

#### Key macOS API Details
- **`proc_pidinfo()` with `PROC_PIDTBSDINFO`**: Returns `proc_bsdinfo` struct containing `pbi_ppid` (parent PID)
- **No special permissions required**: Unlike some Windows APIs, querying process info on macOS works without elevation
- **PID 1 is launchd**: Equivalent to Windows System (PID 4) as the process tree root

#### Cargo.toml Addition
```toml
[target.'cfg(target_os = "macos")'.dependencies]
libproc = "0.14"
```

### 4. FUSE Request Context Integration

The current FUSE implementation (`crates/spfs-vfs/src/fuse.rs`) does **not** use `Request::pid()`:

```rust
// All callbacks have _req parameter unused:
fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
    // No PID-based routing currently
}
```

**Required Changes for macOS:**

#### New Router-Aware Filesystem (`spfs-vfs/src/macos/router.rs`)
```rust
impl fuser::Filesystem for Router {
    fn lookup(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let caller_pid = req.pid();  // fuser provides this on macOS
        let mount = match self.get_mount_for_pid(caller_pid) {
            Ok(m) => m,
            Err(e) => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        // Delegate to mount-specific logic
        mount.lookup(parent, name, reply);
    }
    
    // Repeat pattern for: read, open, readdir, getattr, etc.
}
```

#### fuser Features (`Cargo.toml`)
```toml
[target.'cfg(target_os = "macos")'.dependencies]
fuser = { version = "0.15.1", features = ["macfuse-4-compat"] }
```

### 5. Mount and Handle Implementation

Each `Mount` holds per-runtime state (`winfsp/mount.rs:39-46`):

```rust
pub struct Mount {
    rt: tokio::runtime::Handle,
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    manifest: spfs::tracking::Manifest,
    next_inode: AtomicU64,
    inodes: DashMap<u64, Arc<Entry<u64>>>,
}
```

**For macOS, the same pattern applies:**
- Pre-allocate inodes at mount time by walking the manifest tree
- Store inode-to-entry mapping in concurrent DashMap
- Support three handle types:

```rust
pub enum Handle {
    /// Seekable local file
    BlobFile {
        entry: Arc<Entry<u64>>,
        file: std::fs::File,
    },
    /// Sequential remote stream (no seeking)
    BlobStream {
        entry: Arc<Entry<u64>>,
        offset: Arc<AtomicU64>,
        stream: Arc<tokio::sync::Mutex<Pin<Box<dyn BlobRead>>>>,
    },
    /// Directory listing
    Tree {
        entry: Arc<Entry<u64>>,
    },
}
```

### 6. CLI Command Structure

The WinFSP CLI pattern (`cmd-winfsp/src/cmd_winfsp.rs`) should be adapted:

#### Service Subcommand
```rust
/// Start the background macFUSE filesystem service
#[derive(Debug, Args)]
struct CmdService {
    #[clap(long, exclusive = true)]
    stop: bool,
    
    #[clap(long, default_value = "127.0.0.1:37737", env = "SPFS_MACFUSE_LISTEN_ADDRESS")]
    listen: SocketAddr,
    
    #[clap(default_value = "/spfs")]
    mountpoint: std::path::PathBuf,
}
```

#### Mount Subcommand
```rust
/// Request a mount for a specific process tree
#[derive(Debug, Args)]
struct CmdMount {
    #[clap(long)]
    root_process: Option<u32>,
    
    #[clap(long, default_value = "127.0.0.1:37737", env = "SPFS_MACFUSE_LISTEN_ADDRESS")]
    service: SocketAddr,
    
    /// The EnvSpec to mount
    #[clap(name = "REF")]
    reference: EnvSpec,
}
```

#### Auto-Spawn Pattern
```rust
async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
    let result = connect_to_service(&self.service).await;
    
    let channel = match result {
        Err(err) if is_connection_refused(&err) => {
            // Spawn service as background process
            let exe = std::env::current_exe()?;
            std::process::Command::new(exe)
                .arg("service")
                .arg("--listen")
                .arg(self.service.to_string())
                .spawn()?;
            
            // Retry connection
            connect_to_service(&self.service).await?
        }
        res => res?,
    };
    
    // Send mount request
    let mut client = VfsServiceClient::new(channel);
    client.mount(Request::new(MountRequest {
        root_pid: get_parent_pid()?,
        env_spec: self.reference.to_string(),
    })).await?;
    
    Ok(0)
}
```

## Key Components for macOS Implementation

### 1. New Crate Structure

```
crates/
  spfs-vfs/src/
    macos/                    # NEW
      mod.rs                  # Module exports
      router.rs               # PID-based routing, fuser::Filesystem impl
      mount.rs                # Per-runtime state, inode management
      handle.rs               # File handle types
      process.rs              # macOS process ancestry (libproc)
    
  spfs-cli/
    cmd-fuse-macos/           # NEW
      Cargo.toml
      src/
        main.rs               # Entry point
        cmd_fuse_macos.rs     # service + mount subcommands
```

### 2. Router Module (`macos/router.rs`)

```rust
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use fuser::Request;

use crate::macos::mount::Mount;
use crate::macos::process::get_parent_pids_macos;

#[derive(Clone)]
pub struct Router {
    repos: Vec<Arc<spfs::storage::RepositoryHandle>>,
    routes: Arc<RwLock<HashMap<u32, Arc<Mount>>>>,
    default: Arc<Mount>,
}

impl Router {
    pub fn new(repos: Vec<Arc<spfs::storage::RepositoryHandle>>) -> spfs::Result<Self> {
        let default = Arc::new(Mount::empty()?);
        Ok(Self {
            repos,
            routes: Arc::new(RwLock::new(HashMap::new())),
            default,
        })
    }
    
    pub async fn mount(&self, root_pid: u32, env_spec: EnvSpec) -> spfs::Result<()> {
        let manifest = self.compute_manifest(&env_spec).await?;
        let mount = Mount::new(self.repos.clone(), manifest)?;
        
        let mut routes = self.routes.write();
        if routes.contains_key(&root_pid) {
            return Err(spfs::Error::RuntimeExists(root_pid.to_string()));
        }
        routes.insert(root_pid, Arc::new(mount));
        Ok(())
    }
    
    fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
        let stack = get_parent_pids_macos(Some(caller_pid as i32))
            .unwrap_or_else(|_| vec![caller_pid as i32]);
        
        let routes = self.routes.read();
        for pid in stack {
            if let Some(mount) = routes.get(&(pid as u32)) {
                return Arc::clone(mount);
            }
        }
        Arc::clone(&self.default)
    }
}

impl fuser::Filesystem for Router {
    fn lookup(&mut self, req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.lookup(parent, name, reply);
    }
    
    fn getattr(&mut self, req: &Request<'_>, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.getattr(ino, reply);
    }
    
    fn read(&mut self, req: &Request<'_>, ino: u64, fh: u64, offset: i64, 
            size: u32, flags: i32, lock_owner: Option<u64>, reply: ReplyData) {
        let mount = self.get_mount_for_pid(req.pid());
        mount.read(ino, fh, offset, size, flags, lock_owner, reply);
    }
    
    // ... implement all other fuser::Filesystem methods with same pattern
}
```

### 3. Process Module (`macos/process.rs`)

```rust
use libproc::proc_pid::pidinfo;
use libproc::bsd_info::BSDInfo;

/// Get the process ancestry chain from a given PID up to launchd
/// Returns [pid, parent_pid, grandparent_pid, ..., 1]
pub fn get_parent_pids_macos(root: Option<i32>) -> Result<Vec<i32>, String> {
    let mut current = match root {
        Some(pid) => pid,
        None => std::process::id() as i32,
    };
    
    let mut stack = vec![current];
    const MAX_DEPTH: usize = 100;
    
    for _ in 0..MAX_DEPTH {
        let info: BSDInfo = pidinfo(current, 0)
            .map_err(|e| format!("Failed to get process info: {}", e))?;
        
        let parent = info.pbi_ppid as i32;
        
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
```

### 4. Service Architecture

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
│  │                                                  │    │
│  │  get_mount_for_pid(req.pid()):                  │    │
│  │    1. Get caller PID from fuser Request         │    │
│  │    2. Walk ancestry via libproc                 │    │
│  │    3. Find first registered ancestor            │    │
│  │    4. Return matching Mount or default          │    │
│  └──────────────────────────────────────────────────┘    │
│                                                          │
│  ┌──────────────────────────────────────────────────┐    │
│  │           gRPC Service (tonic)                   │    │
│  │    - mount(root_pid, env_spec)                   │    │
│  │    - shutdown()                                  │    │
│  │    Listen: 127.0.0.1:37737                       │    │
│  └──────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

### 5. Data Flow for Filesystem Operation

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

## Code References

### Existing Code to Adapt
- `crates/spfs-vfs/src/winfsp/router.rs:32-98` - Router pattern with PID mapping
- `crates/spfs-vfs/src/winfsp/mount.rs:39-152` - Mount state and inode management
- `crates/spfs-vfs/src/winfsp/handle.rs:12-75` - Handle types for files/streams/dirs
- `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs:57-315` - CLI service/mount pattern
- `crates/spfs-vfs/src/proto/defs/vfs.proto:1-22` - gRPC proto (reusable)

### Existing FUSE Code to Modify
- `crates/spfs-vfs/src/fuse.rs:768-973` - Filesystem trait impl (add PID routing)
- `crates/spfs-vfs/Cargo.toml:49-50` - Add macfuse-4-compat feature

### New Files Required
- `crates/spfs-vfs/src/macos/mod.rs`
- `crates/spfs-vfs/src/macos/router.rs`
- `crates/spfs-vfs/src/macos/mount.rs`
- `crates/spfs-vfs/src/macos/process.rs`
- `crates/spfs-cli/cmd-fuse-macos/Cargo.toml`
- `crates/spfs-cli/cmd-fuse-macos/src/main.rs`
- `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

## Architecture Insights

### Why a Singleton Router Works

The Router pattern solves the "no namespaces" problem by:
1. **Single mount point**: One macFUSE mount at `/spfs` (or configurable path)
2. **Per-request routing**: Every FUSE operation carries caller PID via `Request`
3. **Process tree inheritance**: Children inherit parent's filesystem view automatically
4. **Concurrent access**: RwLock allows many readers (filesystem ops) with occasional writers (mount registration)

### Trade-offs

| Aspect | Namespace Approach (Linux) | Router Approach (macOS/Windows) |
|--------|----------------------------|--------------------------------|
| Isolation | Kernel-enforced | Application-enforced |
| Security | Strong (separate namespaces) | Weaker (any process can access `/spfs`) |
| Performance | Direct mount | Per-op routing overhead |
| Complexity | OS-level | Application-level |
| Scalability | Per-namespace resources | Shared router state |

### Locking Strategy

The WinFSP code notes a potential improvement (`router.rs:34-35`):
```rust
// TODO: rwlock is not ideal, as we'd like to be able to continue
// uninterrupted when new filesystems are mounted
```

For macOS, consider:
- `DashMap` instead of `RwLock<HashMap>` for lock-free reads
- Epoch-based reclamation for mount removal
- Pre-warming manifest/inode cache before registering

## Historical Context

### Prior Research
- `.llm/shared/research/2025-11-28-spfs-macos-implementation.md` - Initial macOS feasibility analysis
- `.llm/shared/research/2025-11-29-spfs-macos-tahoe-implementation-roadmap.md` - Implementation roadmap
- `.llm/shared/context/2025-11-28-spk-windows-winfsp.md` - WinFSP architecture reference
- `.llm/shared/context/2025-11-28-spk-spfs-fuse.md` - FUSE integration details

### Related Work
- macFUSE: https://osxfuse.github.io/
- fuser crate: https://github.com/cberner/fuser
- libproc crate: https://crates.io/crates/libproc

## Implementation Roadmap

| Phase | Task | Effort |
|-------|------|--------|
| 1 | Create `macos/` module structure | 1 day |
| 2 | Implement `process.rs` with libproc | 1 day |
| 3 | Port Router from WinFSP | 2-3 days |
| 4 | Port Mount/Handle from WinFSP | 2-3 days |
| 5 | Adapt fuse.rs callbacks for routing | 2 days |
| 6 | Create cmd-fuse-macos CLI | 2 days |
| 7 | Integration with spfs runtime | 2-3 days |
| 8 | Testing on macOS hardware | 2-3 days |
| **Total** | | **2-3 weeks** |

## Open Questions

1. **Security Model**: The router-based approach allows any process to access `/spfs`. Is this acceptable, or do we need additional access control (e.g., checking caller UID against runtime owner)?

2. **Mount Cleanup**: How should we handle cleanup when a root_pid exits? Options:
   - Periodic scan for dead PIDs
   - Process exit notification (kevent/kqueue)
   - Lazy cleanup on next access

3. **Performance**: What is the latency impact of process ancestry lookup on every filesystem operation? Should we cache recent lookups?

4. **Concurrent Mounts**: What happens if two processes in different trees access the same inode simultaneously? Need to ensure thread safety in Mount operations.

5. **Write Support**: Is read-only sufficient for initial macOS support, or should write support (scratch directories, copy-on-write) be included from the start?
