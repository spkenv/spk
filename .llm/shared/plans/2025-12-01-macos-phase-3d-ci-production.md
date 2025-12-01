# macOS SPFS Phase 3d: CI/CD and Production Hardening Implementation Plan

## Overview

This plan adds continuous integration for macOS builds and tests, improves error handling and logging, and prepares the implementation for production use. The goal is to catch regressions early and provide better operational visibility.

**Estimated Effort**: 3-4 days

## Current State Analysis

### CI/CD Status

- **Linux CI**: Full build, test, lint, integration tests in `.github/workflows/rust.yml`
- **Windows CI**: Build and `cargo check` only (no full tests)
- **macOS CI**: **Not configured**

### Error Handling Status

- Basic error propagation with `spfs::Error`
- No Sentry integration for macOS-specific errors
- Limited structured logging in some paths

### Production Readiness Gaps

1. No automated testing on macOS
2. No cross-platform regression detection
3. Limited error context in user-facing messages
4. No metrics or observability

## Desired End State

After implementation:

1. macOS builds tested in CI on every PR
2. Both ARM64 and x86_64 architectures tested
3. Helpful error messages for common issues
4. Structured logging throughout
5. Optional Sentry integration for error tracking

### Verification Criteria

**Automated**:
- [ ] CI passes on `macos-latest` (ARM64)
- [ ] CI passes on `macos-13` (x86_64)
- [ ] No regressions in Linux/Windows builds

**Manual**:
- [ ] Error messages are helpful and actionable
- [ ] Logs contain sufficient context for debugging

## What We're NOT Doing

1. **Integration tests requiring macFUSE**: GitHub runners don't have macFUSE
2. **Performance benchmarks in CI**: Too variable on shared runners
3. **Release builds/packaging**: Separate effort for distribution

---

## Task 3d.1: Add macOS CI Workflow

**Effort**: 1 day
**Dependencies**: None

**File**: `.github/workflows/rust.yml`

Add macOS build jobs to the existing workflow:

```yaml
# Add after the build-windows job (around line 50)

build-macos-arm64:
  name: macOS Build (ARM64)
  runs-on: macos-latest  # ARM64 (M1/M2/M3)
  steps:
    - uses: actions/checkout@v4
    
    - name: Setup Rust toolchain
      run: |
        rustup show
        rustup component add clippy
        rustup component add rustfmt --toolchain nightly
    
    - name: Install Dependencies
      run: |
        brew install protobuf flatbuffers
    
    - name: Setup sccache
      uses: mozilla-actions/sccache-action@v0.0.3
    
    - name: Cache cargo registry
      uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
        key: ${{ runner.os }}-arm64-cargo-${{ hashFiles('**/Cargo.lock') }}
        restore-keys: |
          ${{ runner.os }}-arm64-cargo-
    
    - name: Cargo Check (all targets)
      env:
        SCCACHE_GHA_ENABLED: "true"
        RUSTC_WRAPPER: "sccache"
      run: |
        cargo check --workspace --features macfuse-backend
    
    - name: Cargo Check (tests)
      env:
        SCCACHE_GHA_ENABLED: "true"
        RUSTC_WRAPPER: "sccache"
      run: |
        cargo check --workspace --tests --features macfuse-backend
    
    - name: Unit Tests (no FUSE)
      env:
        SCCACHE_GHA_ENABLED: "true"
        RUSTC_WRAPPER: "sccache"
      run: |
        # Run tests that don't require macFUSE
        cargo test -p spfs-vfs --features macfuse-backend --lib
        cargo test -p spfs-encoding
        cargo test -p spk-schema
    
    - name: Clippy
      env:
        SCCACHE_GHA_ENABLED: "true"
        RUSTC_WRAPPER: "sccache"
      run: |
        cargo clippy --workspace --features macfuse-backend -- -D warnings
    
    - name: Format Check
      run: |
        cargo +nightly fmt --all -- --check

build-macos-x86:
  name: macOS Build (x86_64)
  runs-on: macos-13  # Intel
  steps:
    - uses: actions/checkout@v4
    
    - name: Setup Rust toolchain
      run: |
        rustup show
    
    - name: Install Dependencies
      run: |
        brew install protobuf flatbuffers
    
    - name: Setup sccache
      uses: mozilla-actions/sccache-action@v0.0.3
    
    - name: Cache cargo registry
      uses: actions/cache@v3
      with:
        path: |
          ~/.cargo/registry
          ~/.cargo/git
        key: ${{ runner.os }}-x86-cargo-${{ hashFiles('**/Cargo.lock') }}
        restore-keys: |
          ${{ runner.os }}-x86-cargo-
    
    - name: Cargo Check
      env:
        SCCACHE_GHA_ENABLED: "true"
        RUSTC_WRAPPER: "sccache"
      run: |
        cargo check --workspace --features macfuse-backend
    
    - name: Unit Tests (no FUSE)
      env:
        SCCACHE_GHA_ENABLED: "true"
        RUSTC_WRAPPER: "sccache"
      run: |
        cargo test -p spfs-vfs --features macfuse-backend --lib
```

### Success Criteria

#### Automated Verification:
- [ ] CI workflow runs on PRs
- [ ] ARM64 job passes
- [ ] x86_64 job passes
- [ ] Existing Linux/Windows jobs unaffected

---

## Task 3d.2: Add Makefile.macos

**Effort**: 0.5 days
**Dependencies**: None

**File**: `Makefile.macos` (new)

Create macOS-specific Makefile targets:

```makefile
# Makefile.macos - macOS-specific build targets

# macOS doesn't need setcap, but we need macFUSE installed
.PHONY: check-macfuse
check-macfuse:
	@if ! kextstat | grep -q macfuse; then \
		echo "ERROR: macFUSE kernel extension not loaded"; \
		echo "Install with: brew install --cask macfuse"; \
		echo "Then enable in System Settings > Privacy & Security"; \
		exit 1; \
	fi

# Build macOS-specific binaries
spfs_packages := $(spfs_packages),spfs-cli-fuse-macos

# Install target (no capability bits on macOS)
.PHONY: install-macos
install-macos: build
	mkdir -p $(DESTDIR)$(bindir)
	cp target/release/spfs $(DESTDIR)$(bindir)/
	cp target/release/spfs-fuse-macos $(DESTDIR)$(bindir)/
	cp target/release/spk $(DESTDIR)$(bindir)/
	@echo "Installed to $(DESTDIR)$(bindir)"
	@echo "Note: You may need to create /spfs: sudo mkdir -p /spfs"

# Development build with macfuse feature
.PHONY: build-macos
build-macos:
	cargo build --features macfuse-backend

# Run tests that don't require macFUSE
.PHONY: test-macos-unit
test-macos-unit:
	cargo test -p spfs-vfs --features macfuse-backend --lib
	cargo test -p spfs --lib

# Run all tests (requires macFUSE)
.PHONY: test-macos
test-macos: check-macfuse
	cargo test -p spfs-vfs --features macfuse-backend
	cargo test -p spfs
```

Update main `Makefile` to include macOS:

```makefile
# Around line 16, update platform detection
ifeq ($(shell uname -s),Darwin)
include Makefile.macos
FEATURES := macfuse-backend
else ifeq ($(OS),Windows_NT)
include Makefile.windows
else
include Makefile.linux
endif
```

### Success Criteria

#### Automated Verification:
- [ ] `make build-macos` works on macOS
- [ ] `make test-macos-unit` runs tests
- [ ] Platform detection works correctly

---

## Task 3d.3: Improve Error Messages

**Effort**: 0.5 days
**Dependencies**: None

**File**: `crates/spfs/src/error.rs`

Add macOS-specific error variants:

```rust
/// Errors specific to macOS FUSE operations
#[derive(Debug, thiserror::Error)]
pub enum MacOsError {
    #[error("macFUSE is not installed. Install with: brew install --cask macfuse")]
    MacFuseNotInstalled,
    
    #[error("macFUSE kernel extension not loaded. Enable in System Settings > Privacy & Security, then restart")]
    MacFuseNotLoaded,
    
    #[error("SPFS service not running. It should start automatically, but you can start manually with: spfs-fuse-macos service /spfs")]
    ServiceNotRunning,
    
    #[error("Failed to connect to SPFS service at {addr}: {source}")]
    ServiceConnectionFailed {
        addr: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    
    #[error("/spfs mount point does not exist. Create with: sudo mkdir -p /spfs")]
    MountPointMissing,
    
    #[error("Process {pid} not found - it may have already exited")]
    ProcessNotFound { pid: u32 },
}
```

**File**: `crates/spfs/src/env_macos.rs`

Use helpful error messages:

```rust
pub async fn ensure_service_running() -> Result<()> {
    // ... existing code ...
    
    // If we can't start the service, provide helpful error
    match start_service_background().await {
        Ok(()) => { /* ... */ }
        Err(e) => {
            // Check if macFUSE is installed
            if !is_macfuse_available() {
                return Err(MacOsError::MacFuseNotInstalled.into());
            }
            
            // Check if kernel extension is loaded
            if !is_macfuse_loaded() {
                return Err(MacOsError::MacFuseNotLoaded.into());
            }
            
            // Check if mount point exists
            if !std::path::Path::new(SPFS_DIR).exists() {
                return Err(MacOsError::MountPointMissing.into());
            }
            
            return Err(e);
        }
    }
}

fn is_macfuse_available() -> bool {
    // Check if macfuse binaries exist
    std::path::Path::new("/Library/Filesystems/macfuse.fs").exists()
}

fn is_macfuse_loaded() -> bool {
    // Check if kernel extension is loaded
    std::process::Command::new("kextstat")
        .args(["-l", "-b", "io.macfuse.filesystems.macfuse"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo check -p spfs` passes
- [ ] Error types are well-formed

#### Manual Verification:
- [ ] Error messages include actionable instructions
- [ ] Common issues have specific error messages

---

## Task 3d.4: Add Structured Logging

**Effort**: 0.5 days
**Dependencies**: None

**File**: `crates/spfs-vfs/src/macos/router.rs`

Enhance logging with structured fields:

```rust
use tracing::{debug, info, warn, error, instrument};

impl Router {
    #[instrument(skip(self), fields(root_pid, env_spec = %env_spec))]
    async fn mount_internal(
        &self,
        root_pid: u32,
        env_spec: EnvSpec,
        editable: bool,
        runtime_name: Option<String>,
    ) -> spfs::Result<()> {
        debug!("computing environment manifest");
        
        // ... existing code ...
        
        info!(
            root_pid,
            env_spec = %env_spec,
            editable,
            runtime_name = runtime_name.as_deref().unwrap_or("none"),
            "mount registered"
        );
        
        Ok(())
    }
    
    fn get_mount_for_pid(&self, caller_pid: u32) -> Arc<Mount> {
        let ancestry = get_parent_pids_macos(Some(caller_pid as i32))
            .unwrap_or_else(|e| {
                warn!(caller_pid, error = %e, "failed to get process ancestry");
                vec![caller_pid as i32]
            });
        
        // ... rest of implementation ...
    }
}
```

**File**: `crates/spfs-vfs/src/macos/service.rs`

Add request logging:

```rust
#[instrument(skip_all, fields(root_pid = request.get_ref().root_pid))]
async fn mount(
    &self,
    request: Request<proto::MountRequest>,
) -> std::result::Result<Response<proto::MountResponse>, Status> {
    let inner = request.into_inner();
    debug!(
        env_spec = %inner.env_spec,
        editable = inner.editable,
        "mount request received"
    );
    
    // ... existing implementation ...
}
```

### Success Criteria

#### Automated Verification:
- [ ] `cargo check -p spfs-vfs --features macfuse-backend` passes
- [ ] No clippy warnings about unused variables in log macros

#### Manual Verification:
- [ ] Logs show structured fields
- [ ] Important operations are logged at appropriate levels

---

## Task 3d.5: Add Health Check Endpoint

**Effort**: 0.5 days
**Dependencies**: Task 3b.4 (Status Endpoint)

**File**: `crates/spfs-vfs/src/proto/defs/vfs.proto`

Add health check RPC:

```protobuf
message HealthCheckRequest {}

message HealthCheckResponse {
    bool healthy = 1;
    string version = 2;
    uint64 uptime_seconds = 3;
}

service VfsService {
    rpc Mount(MountRequest) returns (MountResponse);
    rpc Shutdown(ShutdownRequest) returns (ShutdownResponse);
    rpc Status(StatusRequest) returns (StatusResponse);
    rpc HealthCheck(HealthCheckRequest) returns (HealthCheckResponse);  // NEW
}
```

**File**: `crates/spfs-vfs/src/macos/service.rs`

```rust
use std::time::Instant;

pub struct Service {
    config: Config,
    router: Arc<Router>,
    start_time: Instant,
}

impl Service {
    pub async fn new(config: Config) -> spfs::Result<Arc<Self>> {
        // ... existing initialization ...
        
        Ok(Arc::new(Self {
            config,
            router,
            start_time: Instant::now(),
        }))
    }
}

#[tonic::async_trait]
impl proto::vfs_service_server::VfsService for Arc<Service> {
    async fn health_check(
        &self,
        _request: Request<proto::HealthCheckRequest>,
    ) -> std::result::Result<Response<proto::HealthCheckResponse>, Status> {
        Ok(Response::new(proto::HealthCheckResponse {
            healthy: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }))
    }
}
```

**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

Add health check to status command output:

```rust
impl CmdStatus {
    async fn run(&self, _config: &spfs::Config) -> Result<i32> {
        // ... existing connection code ...
        
        // Get health check
        let health = client.health_check(proto::HealthCheckRequest {})
            .await
            .into_diagnostic()?
            .into_inner();
        
        println!("Service Status:");
        println!("  Version: {}", health.version);
        println!("  Uptime: {} seconds", health.uptime_seconds);
        println!("  Healthy: {}", health.healthy);
        println!();
        
        // ... existing status output ...
    }
}
```

### Success Criteria

#### Automated Verification:
- [ ] Proto regenerates successfully
- [ ] `spfs-fuse-macos status` shows health info

---

## Task 3d.6: Documentation Updates

**Effort**: 0.5 days
**Dependencies**: All previous tasks

**File**: `docs/spfs/macos-getting-started.md` (new or update existing)

```markdown
# SPFS on macOS Getting Started Guide

## Prerequisites

### 1. Install macFUSE

```bash
brew install --cask macfuse
```

After installation, you need to enable the kernel extension:
1. Open **System Settings** > **Privacy & Security**
2. Scroll down to find the blocked kernel extension
3. Click **Allow** for macFUSE
4. Restart your Mac

### 2. Create Mount Point

```bash
sudo mkdir -p /spfs
```

### 3. Install SPFS

```bash
# From release package
tar -xzf spfs-macos-arm64.tar.gz
sudo cp spfs spfs-fuse-macos /usr/local/bin/

# Or build from source
make build-macos
sudo make install-macos
```

## Usage

### Basic Usage

SPFS automatically manages the background service:

```bash
# Run a command in an SPFS environment
spfs run my-package/1.0.0 -- ls /spfs

# Start an interactive shell
spfs shell my-package/1.0.0

# Edit mode (changes can be committed)
spfs shell --edit my-package/1.0.0
```

### Service Management

The service starts automatically, but you can manage it manually:

```bash
# Check service status
spfs-fuse-macos status

# Stop the service
spfs-fuse-macos service --stop

# Start manually (for debugging)
spfs-fuse-macos service /spfs
```

## Troubleshooting

### "macFUSE is not installed"

Install macFUSE:
```bash
brew install --cask macfuse
```

### "macFUSE kernel extension not loaded"

1. Open System Settings > Privacy & Security
2. Allow the macFUSE kernel extension
3. Restart your Mac

### "/spfs mount point does not exist"

Create the mount point:
```bash
sudo mkdir -p /spfs
```

### Service won't start

Check for detailed errors:
```bash
spfs-fuse-macos service /spfs
```

### Performance Issues

Check cache statistics:
```bash
spfs-fuse-macos status
```

The cache hit rate should be >90%. If lower, report a bug.

## Architecture

See [macOS FUSE Architecture](develop/macos-fuse-architecture.md) for technical details.
```

### Success Criteria

#### Manual Verification:
- [ ] Documentation is complete and accurate
- [ ] All commands in documentation work
- [ ] Troubleshooting covers common issues

---

## Phase 3d Success Criteria Summary

### Automated Verification:
- [ ] macOS ARM64 CI job passes
- [ ] macOS x86_64 CI job passes
- [ ] Linux CI job unaffected
- [ ] Windows CI job unaffected
- [ ] All new code compiles without warnings

### Manual Verification:
- [ ] Error messages are helpful and actionable
- [ ] Logs contain sufficient debugging context
- [ ] Documentation is complete
- [ ] Status command shows useful information

---

## Dependencies

```
Task 3d.1 (CI Workflow) ──────────────────────────────────────────┐
                                                                   │
Task 3d.2 (Makefile.macos) ───────────────────────────────────────│
                                                                   │
Task 3d.3 (Error Messages) ───────────────────────────────────────┼──► Task 3d.6 (Documentation)
                                                                   │
Task 3d.4 (Structured Logging) ───────────────────────────────────│
                                                                   │
Task 3d.5 (Health Check) ─────────────────────────────────────────┘
        (depends on 3b.4)
```

---

## References

- Existing CI: `.github/workflows/rust.yml`
- Linux Makefile: `Makefile.linux`
- Windows Makefile: `Makefile.windows`
- Error handling: `crates/spfs/src/error.rs`
