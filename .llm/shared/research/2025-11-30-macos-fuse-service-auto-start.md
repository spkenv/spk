---
date: 2025-11-30T15:45:00-08:00
researcher: Claude
git_commit: d2855f085f4ebaa50a2241c013c88c3187cd6076
branch: main
repository: spk
topic: "macOS SPFS Service Auto-Start Implementation"
tags: [research, codebase, macos, fuse, service, auto-start, daemon]
status: complete
last_updated: 2025-11-30
last_updated_by: Claude
---

# Research: macOS SPFS Service Auto-Start Implementation

**Date**: 2025-11-30T15:45:00-08:00
**Researcher**: Claude
**Git Commit**: d2855f085f4ebaa50a2241c013c88c3187cd6076
**Branch**: main
**Repository**: spk

## Research Question

How can we implement auto-start behavior for the `spfs-fuse-macos` service so users don't need to manually start it before using SPFS on macOS?

## Summary

The research identifies **three implementation approaches** for auto-starting the macFUSE service:

1. **On-Demand Start** (Recommended): Modify `env_macos.rs` to check if the service is running and spawn it automatically before mounting
2. **LaunchAgent**: Use macOS's launchd system to run the service at login or on-demand via socket activation
3. **Hybrid**: Combine on-demand spawning with optional LaunchAgent for users who prefer always-on

The **On-Demand Start** approach is recommended because:
- It requires no installation/configuration step
- Resources are only used when SPFS is actively needed
- It matches the existing Windows WinFSP pattern
- It provides a "just works" user experience

## Detailed Findings

### Current State: Manual Service Start Required

Currently, users must manually start the service before using SPFS:

```bash
# Terminal 1: Start service
spfs-fuse-macos service /spfs

# Terminal 2: Use SPFS
spfs shell my-package/1.0.0
```

If the service is not running, the mount command fails with:
```
Service is not running. Start it with: spfs-fuse-macos service
```

**Location**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs:202-203`

### Approach 1: On-Demand Start (Recommended)

#### Implementation Location

The primary change would be in `crates/spfs/src/env_macos.rs`, modifying the `mount_fuse_onto()` function to ensure the service is running before attempting to mount.

#### Existing Pattern: Windows WinFSP

Windows already implements this pattern at `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs:268-291`:

```rust
let channel = match result {
    Err(err) if is_connection_refused(&err) => {
        // Service not running, spawn it as detached process
        let exe = std::env::current_exe()...;
        let mut cmd = std::process::Command::new(exe);
        cmd.creation_flags(DETACHED_PROCESS.0)  // Windows-specific detach
            .arg("service")
            .arg("--listen")
            .arg(self.service.to_string())
            .arg(&self.mountpoint);
        let _child = cmd.spawn()...;
        // Then immediately try to connect again
        tonic::transport::Endpoint::from_shared(...)
            .connect()
            .await
    }
    res => res.into_diagnostic()?,
};
```

#### macOS Equivalent Implementation

For macOS, the detachment would use `nix::unistd::setsid()` instead of Windows `DETACHED_PROCESS`. This pattern already exists in the codebase at `crates/spfs/src/monitor.rs:78-88`:

```rust
#[cfg(target_os = "macos")]
unsafe {
    cmd.pre_exec(|| {
        nix::unistd::setsid()
            .map(|_| ())
            .map_err(|err| std::io::Error::from_raw_os_error(err as i32))
    });
}
```

#### Proposed Code Changes

**File**: `crates/spfs/src/env_macos.rs`

Add new helper functions before `mount_fuse_onto()`:

```rust
use std::time::Duration;

const MACFUSE_SERVICE_ADDR: &str = "127.0.0.1:37738";

/// Check if the macFUSE service is running by attempting a gRPC connection
async fn service_is_running() -> bool {
    let endpoint = format!("http://{}", MACFUSE_SERVICE_ADDR);
    tokio::time::timeout(
        Duration::from_millis(100),
        async {
            tonic::transport::Endpoint::from_shared(endpoint)
                .ok()?
                .connect()
                .await
                .ok()
        }
    ).await.unwrap_or(None).is_some()
}

/// Start the macFUSE service in the background if not already running
async fn ensure_service_running() -> Result<()> {
    const MAX_RETRIES: u32 = 5;
    
    for attempt in 0..MAX_RETRIES {
        if service_is_running().await {
            return Ok(());
        }
        
        // Try to start service
        match start_service_background() {
            Ok(()) => {
                // Wait for service to be ready
                return wait_for_service_ready(Duration::from_secs(5)).await;
            }
            Err(e) if attempt < MAX_RETRIES - 1 => {
                // Another process may have started it, retry health check
                let backoff = Duration::from_millis(100 * (1 << attempt));
                tokio::time::sleep(backoff).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    
    Err(Error::String("Could not start or connect to macFUSE service".into()))
}

/// Spawn the service as a detached background process
fn start_service_background() -> Result<()> {
    let spfs_fuse = super::resolve::which_spfs("fuse-macos")
        .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;
    
    let mut cmd = std::process::Command::new(spfs_fuse);
    cmd.arg("service")
        .arg(SPFS_DIR)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    
    // Detach the process so it outlives the parent
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(|err| std::io::Error::from_raw_os_error(err as i32))
        });
    }
    
    cmd.spawn()
        .map_err(|e| Error::process_spawn_error("spfs-fuse-macos service", e, None))?;
    
    Ok(())
}

/// Wait for the service to become ready
async fn wait_for_service_ready(timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if service_is_running().await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(Error::String("macFUSE service did not start in time".into()))
}
```

Then modify `mount_fuse_onto()` to call `ensure_service_running()` first:

```rust
#[cfg(feature = "fuse-backend")]
async fn mount_fuse_onto<P>(&self, rt: &runtime::Runtime, path: P) -> Result<()>
where
    P: AsRef<std::ffi::OsStr>,
{
    // NEW: Ensure service is running before attempting mount
    ensure_service_running().await?;
    
    // ... existing implementation continues ...
}
```

#### Race Condition Handling

The implementation includes race condition handling:

1. Check if service is running (quick gRPC health check)
2. If not, attempt to start service
3. If start fails (e.g., address already in use), assume another process started it
4. Retry health check with exponential backoff
5. Proceed once service confirmed running

#### Dependencies Required

Add to `crates/spfs/Cargo.toml`:
```toml
[target.'cfg(target_os = "macos")'.dependencies]
tonic = { workspace = true }  # For gRPC health check
```

### Approach 2: LaunchAgent

A LaunchAgent plist allows macOS to manage the service lifecycle via `launchd`.

#### Basic LaunchAgent (Always Running)

**File**: `~/Library/LaunchAgents/ai.spk.spfs-fuse-macos.plist`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.spk.spfs-fuse-macos</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/spfs-fuse-macos</string>
        <string>service</string>
        <string>/spfs</string>
    </array>
    
    <key>RunAtLoad</key>
    <true/>
    
    <key>KeepAlive</key>
    <true/>
    
    <key>StandardOutPath</key>
    <string>/tmp/spfs-fuse-macos.log</string>
    
    <key>StandardErrorPath</key>
    <string>/tmp/spfs-fuse-macos.err</string>
</dict>
</plist>
```

**Installation**:
```bash
cp ai.spk.spfs-fuse-macos.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/ai.spk.spfs-fuse-macos.plist
```

**Uninstallation**:
```bash
launchctl unload ~/Library/LaunchAgents/ai.spk.spfs-fuse-macos.plist
rm ~/Library/LaunchAgents/ai.spk.spfs-fuse-macos.plist
```

#### Socket-Activated LaunchAgent (On-Demand)

This variant only starts the service when something connects to the port:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" 
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>ai.spk.spfs-fuse-macos</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/spfs-fuse-macos</string>
        <string>service</string>
        <string>/spfs</string>
    </array>
    
    <key>Sockets</key>
    <dict>
        <key>Listeners</key>
        <dict>
            <key>SockServiceName</key>
            <string>37738</string>
            <key>SockType</key>
            <string>stream</string>
            <key>SockFamily</key>
            <string>IPv4</string>
        </dict>
    </dict>
    
    <key>StandardOutPath</key>
    <string>/tmp/spfs-fuse-macos.log</string>
    
    <key>StandardErrorPath</key>
    <string>/tmp/spfs-fuse-macos.err</string>
</dict>
</plist>
```

**Note**: Socket activation requires the service to accept the socket from launchd via `launch_activate_socket()`. This would require changes to the gRPC server initialization.

#### Pros and Cons

| Aspect | LaunchAgent | On-Demand Start |
|--------|-------------|-----------------|
| Installation required | Yes | No |
| Always running | Yes (or socket-activated) | Only when needed |
| Crash recovery | Automatic via KeepAlive | Must re-run command |
| Multi-user support | Per-user agent | Per-user process |
| Complexity | Configuration file | Code changes |

### Approach 3: Hybrid

Provide both options:
1. Default: On-demand start (no setup required)
2. Optional: LaunchAgent installation for users who prefer always-on

Add CLI commands:
```bash
# Install LaunchAgent for always-on service
spfs-fuse-macos install-launchagent

# Remove LaunchAgent
spfs-fuse-macos uninstall-launchagent
```

## Code References

### Existing Patterns

- `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs:268-291` - Windows auto-start pattern
- `crates/spfs/src/monitor.rs:78-88` - macOS `setsid()` for process detachment
- `crates/spfs/src/env.rs:541-607` - Linux FUSE mount spawning
- `crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs:251-258` - Linux daemonization

### Files to Modify

- `crates/spfs/src/env_macos.rs:127-180` - Add `ensure_service_running()` call
- `crates/spfs/Cargo.toml` - Add tonic dependency for macOS

### gRPC Service Details

- **Default port**: `127.0.0.1:37738`
- **Environment variable**: `SPFS_MACFUSE_LISTEN_ADDRESS`
- **Protocol**: `crates/spfs-vfs/src/proto/defs/vfs.proto`

## Architecture Insights

### Process Flow Comparison

| Platform | Service Model | Process Isolation | Auto-Start |
|----------|---------------|-------------------|------------|
| Linux | Per-runtime FUSE process | Mount namespaces | N/A (different model) |
| Windows | Singleton WinFSP service | PID-based routing | Yes (DETACHED_PROCESS) |
| macOS | Singleton macFUSE service | PID-based routing | No (currently) |

### Service Lifecycle

```
User runs "spfs shell my-package"
         │
         ▼
    env_macos.rs
    mount_fuse_onto()
         │
         ├─── NEW: ensure_service_running()
         │         │
         │         ├── service_is_running()? → Yes → continue
         │         │         │
         │         │         └── No → start_service_background()
         │         │                        │
         │         │                        └── setsid() + spawn
         │         │
         │         └── wait_for_service_ready()
         │
         ▼
    Spawn "spfs-fuse-macos mount ..."
         │
         ▼
    gRPC MountRequest to service
         │
         ▼
    Router registers PID → Mount
         │
         ▼
    /spfs visible to process tree
```

## Historical Context (from .llm/)

- `.llm/shared/context/2025-11-30-macos-service-auto-start-design.md` - Previous design analysis recommending on-demand start
- `.llm/shared/context/2025-11-28-spk-spfs-fuse.md` - FUSE architecture overview
- `.llm/shared/plans/2025-11-30-spfs-macos-fuse-with-scratch.md` - Phase 2 implementation plan (write support)

## Related Research

- `docs/spfs/develop/macos-fuse-architecture.md` - Architecture documentation
- `docs/spfs/macos-getting-started.md` - User-facing getting started guide

## Recommendation

**Implement Approach 1 (On-Demand Start)** for these reasons:

1. **Best UX**: No setup required, "just works"
2. **Security**: Service only runs when user actively using SPFS
3. **Resource efficiency**: No wasted resources when SPFS not in use
4. **Consistency**: Matches Windows WinFSP pattern
5. **Simplicity**: No installation/uninstallation complexity

**Implementation Steps**:

1. Add `ensure_service_running()` and helper functions to `env_macos.rs`
2. Add tonic dependency for gRPC health check
3. Modify `mount_fuse_onto()` to call `ensure_service_running()`
4. Update documentation to reflect auto-start behavior
5. Add integration tests for auto-start scenarios

**Future Enhancements (Phase 3)**:

- Idle timeout: Service exits after 30 minutes of no active mounts
- Optional LaunchAgent for users who prefer always-on
- Smart process monitoring for auto-exit when all runtimes terminate

## Open Questions

1. **Idle timeout**: Should the service auto-exit after a period of inactivity? If so, how long?
2. **Logging**: Where should service logs go when auto-started? Currently `/tmp/spfs-fuse-macos.log` for LaunchAgent.
3. **Error visibility**: If auto-start fails, how do we surface the error to the user?
4. **Multiple users**: How do we handle multiple users on the same machine? (Each user gets their own service on different ports?)
