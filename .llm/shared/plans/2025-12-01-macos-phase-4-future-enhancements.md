# macOS SPFS Phase 4: Future Enhancements

## Overview

This document outlines future enhancements for the macOS SPFS implementation beyond Phase 3. These are lower-priority items that can be implemented as time and resources permit, or as specific needs arise.

**Note**: These are not fully detailed implementation plans. They are design sketches and considerations for future work.

---

## Enhancement 4.1: Automatic Copy-Up on Open

### Problem

Currently, writing to an existing file from the base layer requires manual copy-up:
```bash
# This fails with EROFS
echo "new content" >> /spfs/existing-file

# User must manually copy first
cp /spfs/existing-file /tmp/temp
rm /spfs/existing-file  
cp /tmp/temp /spfs/existing-file
echo "new content" >> /spfs/existing-file
```

### Proposed Solution

Implement automatic copy-up when opening a file with write flags:

```rust
fn open(&mut self, req: &Request<'_>, ino: u64, flags: i32, reply: ReplyOpen) {
    let mount = self.get_mount_for_pid(req.pid());
    
    // Check if write access requested
    let write_requested = (flags & libc::O_WRONLY) != 0 || (flags & libc::O_RDWR) != 0;
    
    if write_requested && mount.is_editable() {
        // Check if file exists in base layer but not in scratch
        if mount.needs_copy_up(ino) {
            // Perform copy-up to scratch
            if let Err(e) = mount.copy_to_scratch(ino) {
                reply.error(libc::EIO);
                return;
            }
        }
    }
    
    mount.open(ino, flags, reply);
}
```

### Considerations

- **Performance**: Copy-up can be slow for large files
- **Atomicity**: Need to handle interrupted copy-ups
- **Storage**: Scratch directory could grow large

### Effort Estimate

2-3 days

---

## Enhancement 4.2: Idle Timeout Service Shutdown

### Problem

Once started, the service runs indefinitely, consuming resources even when SPFS is not in use.

### Proposed Solution

Implement idle timeout (Option 2b from the design doc):

```rust
impl Service {
    async fn idle_monitor_loop(&self) {
        const IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60); // 30 minutes
        
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            
            if self.router.mount_count() == 0 {
                let idle_time = self.last_activity.elapsed();
                if idle_time > IDLE_TIMEOUT {
                    tracing::info!("no active mounts for {} minutes, shutting down", 
                        idle_time.as_secs() / 60);
                    self.shutdown().await;
                    break;
                }
            }
        }
    }
}
```

### Considerations

- **User preference**: Some users may prefer always-on
- **Startup latency**: Users may notice delay after idle shutdown
- **Configuration**: Should be configurable via env var or config

### Effort Estimate

1 day

---

## Enhancement 4.3: LaunchAgent Option for Always-On

### Problem

Some users (especially in CI/CD) may prefer the service to always be available without startup delay.

### Proposed Solution

Provide a LaunchAgent plist that users can optionally install:

```xml
<!-- ai.spk.spfs-fuse-macos.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" ...>
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
    <string>/usr/local/var/log/spfs-fuse-macos.log</string>
    <key>StandardErrorPath</key>
    <string>/usr/local/var/log/spfs-fuse-macos.err</string>
</dict>
</plist>
```

Add CLI commands:
```bash
spfs-fuse-macos launchagent install   # Copy plist, load service
spfs-fuse-macos launchagent uninstall # Unload service, remove plist
spfs-fuse-macos launchagent status    # Check if installed/running
```

### Effort Estimate

1-2 days

---

## Enhancement 4.4: FUSE-T Support

### Problem

macFUSE requires a kernel extension, which:
- Needs explicit enablement on Apple Silicon
- May be deprecated in future macOS versions
- Has licensing considerations

FUSE-T is a userspace-only alternative that doesn't require a kernel extension.

### Research Required

1. Evaluate FUSE-T compatibility with `fuser` crate
2. Benchmark performance comparison
3. Assess feature parity (especially for editable mounts)

### Proposed Approach

If viable, add FUSE-T as an alternative backend:

```rust
pub enum MacFuseBackend {
    MacFuse,  // Traditional macFUSE (default)
    FuseT,    // FUSE-T userspace implementation
}
```

### Considerations

- **Compatibility**: FUSE-T may not support all FUSE operations
- **Performance**: Userspace may be slower than kernel extension
- **Availability**: Need to verify FUSE-T is actively maintained

### Effort Estimate

1-2 weeks (including research)

---

## Enhancement 4.5: FSKit Migration Path

### Problem

Apple is moving toward FSKit for filesystem extensions, deprecating kernel extensions.

### Research Required

1. Monitor FSKit development and API stability
2. Evaluate when FSKit supports per-request caller context
3. Plan migration when FSKit is viable

### Current FSKit Limitations

- No per-request caller context (can't implement PID-based routing)
- Still in early development as of macOS 15

### Proposed Approach

When FSKit matures:
1. Create `crates/spfs-vfs/src/fskit/` module
2. Implement FSKit backend if it supports PID context
3. Default to FSKit on macOS versions that support it

### Effort Estimate

Unknown - depends on Apple's FSKit roadmap

---

## Enhancement 4.6: Durable Runtime Support

### Problem

Currently, macOS only supports transient runtimes. Durable runtimes (persist across reboots) are not implemented.

### Proposed Solution

Implement durable runtime support:

1. **Persist scratch directory**: Use a persistent location instead of `/tmp`
2. **Runtime state storage**: Store runtime metadata in persistent storage
3. **Service startup restoration**: On service start, restore previous durable runtimes

### Considerations

- **Storage location**: `~/Library/Application Support/spfs/runtimes/`?
- **Cleanup**: Need explicit deletion of durable runtimes
- **Mount order**: Handle mount dependencies

### Effort Estimate

3-5 days

---

## Enhancement 4.7: Performance Benchmarking Suite

### Problem

No automated benchmarks to track performance regressions.

### Proposed Solution

Create a benchmarking suite:

```rust
// crates/spfs-vfs/benches/macos_bench.rs

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_ancestry_lookup(c: &mut Criterion) {
    c.bench_function("ancestry_lookup", |b| {
        b.iter(|| {
            get_parent_pids_macos(Some(std::process::id() as i32))
        })
    });
}

fn bench_route_lookup(c: &mut Criterion) {
    let router = /* setup */;
    c.bench_function("route_lookup", |b| {
        b.iter(|| {
            router.get_mount_for_pid(12345)
        })
    });
}

criterion_group!(benches, bench_ancestry_lookup, bench_route_lookup);
criterion_main!(benches);
```

### Effort Estimate

1-2 days

---

## Enhancement 4.8: Sentry Integration

### Problem

Error tracking and monitoring in production deployments.

### Proposed Solution

Add optional Sentry integration:

```rust
#[cfg(feature = "sentry")]
fn init_sentry() {
    let _guard = sentry::init(sentry::ClientOptions {
        dsn: std::env::var("SPFS_SENTRY_DSN").ok(),
        release: Some(env!("CARGO_PKG_VERSION").into()),
        ..Default::default()
    });
}
```

Report errors:
```rust
#[cfg(feature = "sentry")]
fn report_error(error: &Error) {
    sentry::capture_error(error);
}
```

### Effort Estimate

1 day

---

## Enhancement 4.9: Windows-Style File System Driver (Long Term)

### Problem

macFUSE/FUSE-T are userspace solutions with inherent performance limitations.

### Research Required

Investigate if a kernel-level solution is viable:
- IOKit/DriverKit filesystem drivers
- Apple's System Extensions framework
- Code signing and notarization requirements

### Considerations

- **Complexity**: Kernel development is significantly more complex
- **Stability**: Kernel bugs crash the system
- **Distribution**: Kernel extensions require special entitlements

### Effort Estimate

Unknown - significant research required

---

## Priority Matrix

| Enhancement | Priority | Effort | Impact | Dependencies |
|-------------|----------|--------|--------|--------------|
| 4.1 Copy-Up on Open | Medium | 2-3 days | High UX | None |
| 4.2 Idle Timeout | Low | 1 day | Medium | Phase 3b |
| 4.3 LaunchAgent | Low | 1-2 days | Low | None |
| 4.4 FUSE-T | Medium | 1-2 weeks | High | Research |
| 4.5 FSKit | Future | Unknown | High | Apple |
| 4.6 Durable Runtimes | Medium | 3-5 days | Medium | Phase 3b |
| 4.7 Benchmarks | Low | 1-2 days | Medium | None |
| 4.8 Sentry | Low | 1 day | Low | None |
| 4.9 Kernel Driver | Future | Significant | High | Research |

---

## Recommended Sequence

If implementing these enhancements:

1. **Short term** (next release):
   - 4.1 Copy-Up on Open (high UX impact)
   - 4.7 Benchmarking Suite (track performance)

2. **Medium term**:
   - 4.4 FUSE-T Support (future-proofing)
   - 4.6 Durable Runtimes (feature parity)

3. **Long term**:
   - 4.5 FSKit Migration (when Apple is ready)
   - 4.9 Kernel Driver (if performance demands)

---

## References

- FUSE-T: https://github.com/macos-fuse-t/fuse-t
- FSKit: https://developer.apple.com/documentation/fskit
- Apple DriverKit: https://developer.apple.com/documentation/driverkit
- Original Phase 3 Plan: `.llm/shared/plans/2025-11-29-spfs-macos-implementation.md`
