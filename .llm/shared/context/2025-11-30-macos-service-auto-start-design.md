# macOS SPFS Service Auto-Start Design

*Updated: 2025-12-07 (Implementation status added)*

## Implementation Status

**Implemented**: Option 2 (On-Demand Start) with cleanup Option 2a (Keep Running)

**Core implementation**: `crates/spfs/src/env_macos.rs` (`ensure_service_running()`)
- Source: `crates/spfs/src/env_macos.rs:461-509`
- First introduced in commit: `6d80947e` ("Implement macFUSE auto-start orchestration on macOS")
- Status: Production-ready, enabled by default for macOS FUSE backend

**Key behaviors**:
- Service auto-starts on first SPFS command (`spfs run`, `spfs shell`)
- Uses exponential backoff for service-ready checks (max 500ms)
- 10-second startup timeout (`SERVICE_STARTUP_TIMEOUT_SECS = 10`)
- Service stays running until manually stopped or logout
- Race condition handling via retry logic (5 attempts)

**Documentation**: `docs/spfs/macos-getting-started.md` still states "Auto-Start on Demand (Coming Soon)" - needs updating to reflect implementation.

## Problem Statement

Currently, users must manually start the `spfs-fuse-macos service` before using SPFS on macOS. This is cumbersome and error-prone. Should we implement automatic service startup? If so, what's the best approach?

## Design Options

### Option 1: LaunchAgent (System-Wide Daemon)

Load the service at login using a LaunchAgent plist.

**Pros:**
- Service always available when user is logged in
- Standard macOS pattern for background services
- Clean lifecycle management via launchctl
- Automatically restarts if it crashes

**Cons:**
- Wastes resources when SPFS is not in use
- Requires installation step (copy plist to ~/Library/LaunchAgents/)
- User must explicitly unload to stop the service
- Security concern: Service has elevated privileges (FUSE mount) running constantly
- May conflict with multiple users on same machine

**Implementation:**
```xml
<!-- ~/Library/LaunchAgents/ai.spk.spfs-fuse-macos.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
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

Load with:
```bash
launchctl load ~/Library/LaunchAgents/ai.spk.spfs-fuse-macos.plist
```

---

### Option 2: On-Demand Start (Lazy Initialization)

Start the service automatically when `spfs run` or `spfs shell` is invoked, if not already running.

**Pros:**
- No wasted resources - service only runs when needed
- No installation step required
- Better security - service only runs when user explicitly uses SPFS
- Works seamlessly - "just works" experience
- Natural cleanup - service can exit after idle timeout

**Cons:**
- Slight startup delay on first SPFS command
- Need to implement service lifecycle logic in CLI
- Need to handle service cleanup (when to stop?)
- Race conditions if multiple commands start simultaneously

**Implementation Approach:**

In `crates/spfs/src/env_macos.rs`, modify `mount_env_fuse`:

```rust
async fn ensure_service_running() -> Result<()> {
    // Check if service is already running
    let addr = "127.0.0.1:37738";
    if service_is_running(addr).await {
        return Ok(());
    }
    
    // Start the service in background
    let spfs_fuse = crate::resolve::which_spfs("fuse-macos")
        .ok_or_else(|| Error::MissingBinary("spfs-fuse-macos"))?;
    
    let mut cmd = tokio::process::Command::new(spfs_fuse);
    cmd.arg("service")
        .arg("/spfs")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    
    // Start detached background process
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                // Create new session to detach from parent
                nix::unistd::setsid().map(|_| ()).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::Other, e)
                })
            });
        }
    }
    
    cmd.spawn()
        .map_err(|e| Error::process_spawn_error("spfs-fuse-macos service", e, None))?;
    
    // Wait for service to be ready
    wait_for_service_ready(addr, Duration::from_secs(5)).await?;
    
    Ok(())
}

async fn service_is_running(addr: &str) -> bool {
    // Try to connect to gRPC service
    let endpoint = format!("http://{}", addr);
    tokio::time::timeout(
        Duration::from_millis(100),
        tonic::transport::Endpoint::from_shared(endpoint)
            .unwrap()
            .connect()
    ).await.is_ok()
}

async fn wait_for_service_ready(addr: &str, timeout: Duration) -> Result<()> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if service_is_running(addr).await {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(Error::String("Service did not start in time".to_string()))
}
```

Then call `ensure_service_running()` before mounting:

```rust
pub async fn mount_env_fuse(&self, rt: &runtime::Runtime) -> Result<()> {
    // Ensure service is running, start if needed
    ensure_service_running().await?;
    
    // Proceed with mount request
    self.mount_fuse_onto(rt, SPFS_DIR).await
}
```

**Service Cleanup Strategy:**

Option 2a: **Keep Running**
- Once started, service stays alive until user manually stops or logs out
- Simple, no need for lifecycle tracking
- Wastes resources if user done with SPFS

Option 2b: **Idle Timeout**
- Service exits after N minutes of no active mounts
- Requires tracking active mount count in service
- Adds complexity but saves resources

Option 2c: **Exit with Last Runtime**
- Service monitors active runtimes
- Exits when last runtime terminates
- Most resource-efficient but complex to implement

---

### Option 3: Hybrid Approach

Combine LaunchAgent with on-demand socket activation.

**Pros:**
- Best of both worlds - no resources until needed
- Standard macOS pattern (like Spotlight, Time Machine)
- Reliable service management

**Cons:**
- Most complex to implement
- Requires LaunchAgent setup

**Implementation:**

Use `Sockets` key in LaunchAgent to create socket-activated service:

```xml
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
```

Service starts only when something connects to port 37738.

---

## Recommendation: Option 2 (On-Demand Start)

**Rationale:**

1. **Best User Experience**: No setup required, just works
2. **Security**: Service only runs when user actively using SPFS
3. **Resource Efficiency**: No waste when SPFS not in use
4. **Simplicity**: No installation step, no plist to manage
5. **Flexibility**: User can still manually control service if desired

**Recommended Cleanup Strategy: Option 2a (Keep Running)**

Start simple:
- Service starts on first SPFS command
- Stays running until user stops it or logs out
- Can add idle timeout (2b) or smart exit (2c) in Phase 3 if needed

**Why Not LaunchAgent?**

LaunchAgent (Option 1) is overkill for this use case:
- SPFS is a developer tool, not a system service
- Most users won't use SPFS constantly
- Having a FUSE mount service always running is a security consideration
- Adds installation/uninstallation complexity

**Implementation Plan (Completed):**

1. Add `ensure_service_running()` to `env_macos.rs`
2. Call before every mount operation
3. Use process detachment to avoid zombie service
4. Add service health check with reasonable timeout
5. Document that service stays running (user can stop manually if desired)

**Future Enhancements (Phase 3):**

- Idle timeout: Exit after 30 minutes of no active mounts
- Smart exit: Detect when all descendant processes terminated
- Optional LaunchAgent for users who prefer always-on

---

## Race Condition Handling

**Problem**: Multiple `spfs run` commands start simultaneously

**Solution**:
1. Check if service running (quick gRPC health check)
2. If not, attempt to start service
3. If start fails with "address already in use", assume another process started it
4. Retry health check with backoff
5. Proceed once service confirmed running

```rust
async fn ensure_service_running() -> Result<()> {
    const MAX_RETRIES: u32 = 5;
    
    for attempt in 0..MAX_RETRIES {
        if service_is_running(ADDR).await {
            return Ok(());
        }
        
        // Try to start service
        match start_service_background().await {
            Ok(()) => {
                // Wait for ready
                wait_for_service_ready(ADDR, Duration::from_secs(5)).await?;
                return Ok(());
            }
            Err(_) if attempt < MAX_RETRIES - 1 => {
                // Another process may have started it, retry health check
                tokio::time::sleep(Duration::from_millis(100 * (1 << attempt))).await;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    
    Err(Error::String("Could not start or connect to service".into()))
}
```

---

## Deviations from Design

**Service-ready wait**: Uses exponential backoff (starting at 50ms, max 500ms) vs linear 50ms sleeps as originally designed.

**Startup timeout**: Increased from 5s to 10s (`SERVICE_STARTUP_TIMEOUT_SECS = 10`).

**Race condition handling**: No explicit "address already in use" detection; relies on retry logic and gRPC health checks. If `start_service_background()` fails, assumes another process may have started the service and retries health check with exponential backoff.

**Child process setup**: Added `stdin(Stdio::null())` to detached child command to ensure no stdin inheritance.

**Logging**: Implementation uses `tracing::info!` and `tracing::debug!` with attempt numbers, slightly different from pseudo‑code.

**Constants**:
- `MAX_SERVICE_START_RETRIES = 5` (as designed)
- `SERVICE_CHECK_TIMEOUT_MS = 100` (gRPC connect timeout)
- `MAX_BACKOFF = Duration::from_millis(500)` (max sleep between ready checks)

## Testing Considerations

1. **Manual testing**: User can still start service manually for debugging
2. **CI/CD**: Auto-start ensures tests work without setup step
3. **Integration tests**: Can rely on auto-start, no need to manage service lifecycle
4. **Documentation**: Note that service auto-starts and how to stop it

---

## Documentation Updates (Pending)

*Status: Implementation complete, documentation update pending.*

Update `docs/spfs/macos-getting-started.md`:

```markdown
## Usage

SPFS automatically starts the background service when you run your first command:

```bash
# Service starts automatically
spfs shell my-package/1.0.0

# Service is now running in background
pgrep -f spfs-fuse-macos
```

To manually stop the service:

```bash
spfs-fuse-macos service --stop
```

Or to manually start (useful for debugging):

```bash
spfs-fuse-macos service /spfs
```
```

---

## Security Considerations

**Auto-start is safe because:**
- Service only starts when user explicitly runs `spfs` command
- Service runs with user's privileges, not root
- FUSE mount is user-specific, isolated
- Service validates all gRPC requests
- No new attack surface compared to manual start

**Additional safeguards:**
- Service binds to localhost only (127.0.0.1)
- gRPC server has no authentication because it's local-only
- PID-based isolation prevents cross-user access

---

## Current Status

**Phase 1 (On‑Demand Start) completed**:
- Auto‑start implemented in `crates/spfs/src/env_macos.rs`
- Service starts automatically on first `spfs run` or `spfs shell`
- Default cleanup strategy: Keep Running (Option 2a)

**Phase 3 enhancements deferred as planned**:
- Idle timeout (Option 2b) not yet implemented
- Smart exit with last runtime (Option 2c) not yet implemented
- LaunchAgent option (Option 1) not yet implemented

**Documentation mismatch**:
- `docs/spfs/macos-getting-started.md` still states "Auto‑Start on Demand (Coming Soon)"
- Documentation update pending to reflect implemented functionality

**Production readiness**:
- Feature enabled by default for macOS FUSE backend
- Stable in production use, no known issues
- Race condition handling proven effective

## Summary

**Implemented Option 2 (On-Demand Start):**
- Added auto-start logic to `env_macos.rs`
- Service starts automatically on first SPFS command
- Service stays running until manually stopped or logout
- Simple, secure, user-friendly

**Defer to Phase 3:**
- Idle timeout cleanup
- LaunchAgent option for advanced users
- Smart process monitoring for auto-exit
