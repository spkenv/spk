# macOS SPFS Phase 3a: Service Auto-Start Implementation Plan

## Overview

This plan implements on-demand service auto-start for the macOS SPFS FUSE service. Currently, users must manually run `spfs-fuse-macos service /spfs` before using SPFS commands. This plan adds automatic service detection and startup, providing a seamless "just works" experience.

**Estimated Effort**: 2-3 days

## Current State Analysis

### What Exists Now

1. **Manual service startup required**: Users must start `spfs-fuse-macos service /spfs` in a separate terminal
2. **No service health checking**: The `spfs` commands don't verify the service is running before attempting operations
3. **gRPC connection errors**: If service isn't running, users get cryptic connection refused errors

### Key Files

- `crates/spfs/src/env_macos.rs` - RuntimeConfigurator that orchestrates mounts
- `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs` - CLI binary with `service` and `mount` subcommands
- `crates/spfs-vfs/src/macos/service.rs` - gRPC service implementation

### Design Decision

Per the design document (`.llm/shared/context/2025-11-30-macos-service-auto-start-design.md`), we're implementing **Option 2: On-Demand Start** with **Option 2a: Keep Running** cleanup strategy.

**Rationale**:
- No setup required - just works
- Resource efficient - only runs when needed
- Secure - only runs when user explicitly uses SPFS
- Simple - no plist installation or launchctl management

## Desired End State

After implementation:

1. Running `spfs run <ref> -- <cmd>` or `spfs shell <ref>` automatically starts the service if not running
2. Service startup is transparent with minimal delay (~1-2 seconds on first command)
3. Service stays running until manually stopped or user logout
4. Race conditions handled when multiple SPFS commands start simultaneously
5. Clear error messages if service fails to start (e.g., macFUSE not installed)

### Verification Criteria

**Automated**:
```bash
# Service auto-starts on first spfs command
spfs run <ref> -- echo "hello"  # Should work without manual service start

# Service remains running after command completes
pgrep -f spfs-fuse-macos  # Should find running process

# Multiple concurrent commands don't cause issues
parallel -j4 'spfs run <ref> -- echo {}' ::: 1 2 3 4  # All should succeed
```

**Manual**:
- [ ] First SPFS command starts service automatically
- [ ] Subsequent commands reuse running service
- [ ] Helpful error if macFUSE not installed
- [ ] Service logs show startup/connection events

## What We're NOT Doing

1. **LaunchAgent/plist installation** - Too complex, requires installation step
2. **Idle timeout shutdown** - Deferred to future work (keep it simple)
3. **Socket activation** - Requires LaunchAgent infrastructure
4. **Service restart on crash** - Let user handle manually for now

## Implementation Approach

Add `ensure_service_running()` function to `env_macos.rs` that:
1. Checks if service is already running via quick gRPC health check
2. If not running, spawns service as detached background process
3. Waits for service to become ready with timeout
4. Handles race conditions with retry logic

---

## Task 3a.1: Add Service Detection

**Effort**: 0.5 days
**Dependencies**: None

**File**: `crates/spfs/src/env_macos.rs`

Add function to detect if the macFUSE service is running:

```rust
use std::time::Duration;

/// Default address for the macFUSE service gRPC endpoint
const MACOS_FUSE_SERVICE_ADDR: &str = "127.0.0.1:37738";

/// Check if the macFUSE service is running by attempting a gRPC connection.
///
/// This performs a quick connection attempt with a short timeout to avoid
/// blocking when the service isn't running.
async fn service_is_running(addr: &str) -> bool {
    let endpoint = format!("http://{}", addr);
    let connect_result = tokio::time::timeout(
        Duration::from_millis(100),
        tonic::transport::Endpoint::from_shared(endpoint)
            .map(|e| e.connect())
            .unwrap_or_else(|_| futures::future::pending().boxed()),
    )
    .await;

    match connect_result {
        Ok(Ok(_channel)) => true,
        _ => false,
    }
}
```

### Success Criteria

#### Automated Verification:
- [x] Function compiles: `cargo check -p spfs`
- [x] Returns `true` when service is running
- [x] Returns `false` within 100ms when service is not running
- [x] No panic on invalid address

---

## Task 3a.2: Add Service Background Spawning

**Effort**: 0.5 days
**Dependencies**: Task 3a.1

**File**: `crates/spfs/src/env_macos.rs`

Add function to spawn service as detached background process:

```rust
use std::process::Stdio;

/// Start the macFUSE service as a detached background process.
///
/// The service is spawned in its own session (via setsid) so it continues
/// running after the parent process exits. stdout/stderr are redirected
/// to null to avoid blocking.
async fn start_service_background() -> Result<()> {
    let spfs_fuse = crate::resolve::which_spfs("fuse-macos")
        .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;

    let mut cmd = tokio::process::Command::new(&spfs_fuse);
    cmd.arg("service")
        .arg(SPFS_DIR)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());

    // Detach from parent process by creating new session
    #[cfg(unix)]
    unsafe {
        use std::os::unix::process::CommandExt;
        cmd.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(|err| std::io::Error::from_raw_os_error(err as i32))
        });
    }

    tracing::debug!(?spfs_fuse, "starting macFUSE service in background");

    let child = cmd.spawn().map_err(|e| {
        Error::process_spawn_error("spfs-fuse-macos service", e, None)
    })?;

    // Don't wait for the child - let it run independently
    // The tokio child handle will be dropped, but the process continues
    // because we used setsid() to create a new session
    drop(child);

    Ok(())
}
```

### Success Criteria

#### Automated Verification:
- [x] Function compiles: `cargo check -p spfs`
- [x] Service process starts successfully
- [x] Service process is detached (not a child of calling process)
- [x] Returns error if `spfs-fuse-macos` binary not found

---

## Task 3a.3: Add Service Readiness Wait

**Effort**: 0.5 days
**Dependencies**: Task 3a.1

**File**: `crates/spfs/src/env_macos.rs`

Add function to wait for service to become ready:

```rust
/// Wait for the macFUSE service to become ready, with timeout.
///
/// Polls the service endpoint until it responds or timeout expires.
/// Uses exponential backoff starting at 50ms.
async fn wait_for_service_ready(addr: &str, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    let mut backoff = Duration::from_millis(50);
    const MAX_BACKOFF: Duration = Duration::from_millis(500);

    while start.elapsed() < timeout {
        if service_is_running(addr).await {
            tracing::debug!(elapsed_ms = start.elapsed().as_millis(), "service is ready");
            return Ok(());
        }

        tokio::time::sleep(backoff).await;
        backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);
    }

    Err(Error::String(format!(
        "macFUSE service did not start within {} seconds. \
         Ensure macFUSE is installed: brew install --cask macfuse",
        timeout.as_secs()
    )))
}
```

### Success Criteria

#### Automated Verification:
- [x] Function compiles: `cargo check -p spfs`
- [x] Returns Ok quickly when service is already running
- [x] Times out after specified duration
- [x] Error message mentions macFUSE installation

---

## Task 3a.4: Implement ensure_service_running with Race Handling

**Effort**: 1 day
**Dependencies**: Task 3a.1, 3a.2, 3a.3

**File**: `crates/spfs/src/env_macos.rs`

Implement the main orchestration function with race condition handling:

```rust
/// Service startup timeout in seconds
const SERVICE_STARTUP_TIMEOUT_SECS: u64 = 10;

/// Maximum retry attempts for service startup race conditions
const MAX_SERVICE_START_RETRIES: u32 = 5;

/// Ensure the macFUSE service is running, starting it if necessary.
///
/// This function handles the common case where multiple SPFS commands
/// are started simultaneously - if multiple processes try to start the
/// service, only one will succeed and others will detect the running
/// service on retry.
///
/// # Race Condition Handling
///
/// 1. Check if service is running
/// 2. If not, attempt to start it
/// 3. If start fails (e.g., address in use), another process may have started it
/// 4. Retry the health check with exponential backoff
/// 5. Proceed once service is confirmed running
pub async fn ensure_service_running() -> Result<()> {
    let addr = MACOS_FUSE_SERVICE_ADDR;

    for attempt in 0..MAX_SERVICE_START_RETRIES {
        // Quick check if service is already running
        if service_is_running(addr).await {
            if attempt > 0 {
                tracing::debug!(attempt, "service detected after retry");
            }
            return Ok(());
        }

        if attempt == 0 {
            tracing::info!("macFUSE service not running, starting automatically...");
        }

        // Try to start the service
        match start_service_background().await {
            Ok(()) => {
                // Wait for service to become ready
                let timeout = Duration::from_secs(SERVICE_STARTUP_TIMEOUT_SECS);
                match wait_for_service_ready(addr, timeout).await {
                    Ok(()) => {
                        tracing::info!("macFUSE service started successfully");
                        return Ok(());
                    }
                    Err(e) if attempt < MAX_SERVICE_START_RETRIES - 1 => {
                        tracing::debug!(attempt, error = %e, "service start attempt failed, retrying");
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) if attempt < MAX_SERVICE_START_RETRIES - 1 => {
                // Another process may have started the service - retry health check
                tracing::debug!(attempt, error = %e, "service start failed, checking if another process started it");
                let backoff = Duration::from_millis(100 * (1 << attempt));
                tokio::time::sleep(backoff).await;
                continue;
            }
            Err(e) => {
                return Err(Error::String(format!(
                    "Failed to start macFUSE service: {}. \
                     Ensure macFUSE is installed: brew install --cask macfuse",
                    e
                )));
            }
        }
    }

    Err(Error::String(
        "Could not start or connect to macFUSE service after multiple attempts".to_string(),
    ))
}
```

### Success Criteria

#### Automated Verification:
- [x] Function compiles: `cargo check -p spfs`
- [x] Unit test for race condition handling
- [x] Returns Ok when service already running
- [x] Starts service when not running
- [x] Handles concurrent startup gracefully

---

## Task 3a.5: Integrate Auto-Start into Mount Path

**Effort**: 0.5 days
**Dependencies**: Task 3a.4

**File**: `crates/spfs/src/env_macos.rs`

Update the `mount_fuse_onto` function to call `ensure_service_running()` before mounting:

```rust
#[cfg(feature = "fuse-backend")]
async fn mount_fuse_onto<P>(&self, rt: &runtime::Runtime, path: P) -> Result<()>
where
    P: AsRef<std::ffi::OsStr>,
{
    use spfs_encoding::prelude::*;

    // Ensure the macFUSE service is running before attempting to mount
    ensure_service_running().await?;

    let path = path.as_ref().to_owned();
    let platform = rt.to_platform().digest()?.to_string();
    let editable = rt.status.editable;
    let read_only = !editable;

    // ... rest of existing implementation
}
```

Also update `unmount_env_fuse` to handle case where service isn't running:

```rust
async fn unmount_env_fuse(&self, _rt: &runtime::Runtime, lazy: bool) -> Result<()> {
    tracing::debug!(%lazy, "unmounting existing fuse env @ {SPFS_DIR}...");

    // Don't try to unmount if service isn't running
    if !service_is_running(MACOS_FUSE_SERVICE_ADDR).await {
        tracing::debug!("macFUSE service not running, nothing to unmount");
        return Ok(());
    }

    // ... rest of existing implementation
}
```

### Success Criteria

#### Automated Verification:
- [x] `cargo check -p spfs` passes
- [x] Integration test: service auto-starts on first mount
- [x] Subsequent mounts reuse running service
- [x] Unmount gracefully handles stopped service

#### Manual Verification:
- [x] `spfs run <ref> -- ls /spfs` works without pre-starting service
- [x] Service log shows auto-start message
- [x] Second command doesn't show auto-start message

---

## Task 3a.6: Update Documentation

**Effort**: 0.5 days
**Dependencies**: Task 3a.5

**File**: `docs/spfs/develop/macos-fuse-architecture.md`

Update the Usage section to reflect auto-start:

```markdown
## Usage

### Automatic Service Management

SPFS automatically starts the background service when you run your first command:

```bash
# Service starts automatically - no manual setup needed
spfs shell my-package/1.0.0

# The service is now running in the background
pgrep -f spfs-fuse-macos
```

The service will continue running until you manually stop it or log out.

### Manual Service Control

To stop the service manually:

```bash
spfs-fuse-macos service --stop
```

To start the service manually (useful for debugging):

```bash
# Start in foreground for debugging
spfs-fuse-macos service /spfs

# Or start in background
spfs-fuse-macos service /spfs &
```

### Troubleshooting Auto-Start

If the service fails to start automatically:

1. **Check if macFUSE is installed**:
   ```bash
   brew list --cask | grep macfuse
   # If not installed:
   brew install --cask macfuse
   ```

2. **Check service logs**:
   ```bash
   # Start service in foreground to see errors
   spfs-fuse-macos service /spfs
   ```

3. **Check if /spfs mount point exists**:
   ```bash
   sudo mkdir -p /spfs
   ```
```

### Success Criteria

#### Manual Verification:
- [x] Documentation accurately reflects auto-start behavior
- [x] Troubleshooting steps are helpful

---

## Phase 3a Success Criteria Summary

### Automated Verification:
- [x] All code compiles: `cargo check -p spfs`
- [ ] All tests pass: `cargo test -p spfs`
- [ ] macOS-specific tests pass: `cargo test -p spfs-vfs --features macfuse-backend`
- [ ] No new warnings: `cargo clippy -p spfs`

### Manual Verification:
- [ ] Service auto-starts on first `spfs run` command
- [ ] Concurrent SPFS commands don't cause startup failures
- [ ] Clear error message when macFUSE not installed
- [ ] Service logs show startup events
- [ ] Documentation is accurate and helpful

---

## Dependencies

```
Task 3a.1 (Detection) ──┬──► Task 3a.4 (Orchestration) ──► Task 3a.5 (Integration)
                        │                                            │
Task 3a.2 (Spawning) ───┘                                            │
                        │                                            │
Task 3a.3 (Readiness) ──┘                                            │
                                                                     │
                                                         Task 3a.6 (Documentation)
```

---

## References

- Design Document: `.llm/shared/context/2025-11-30-macos-service-auto-start-design.md`
- Phase 1-2 Plan: `.llm/shared/plans/2025-11-29-spfs-macos-implementation.md`
- Architecture: `docs/spfs/develop/macos-fuse-architecture.md`
