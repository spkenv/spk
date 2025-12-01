# Fix macOS Binary Conditional Compilation Implementation Plan

## Overview

Add platform-specific conditional compilation guards to the `spfs-cli-fuse-macos` binary to prevent compilation failures on non-macOS platforms. This fix follows the established pattern used by the `spfs-cli-winfsp` binary for Windows.

## Current State Analysis

The `spfs-cli-fuse-macos` binary currently attempts to compile on all platforms, causing build failures on Linux (and likely Windows) because:

1. It unconditionally imports `spfs_vfs::macos` module (line 12 of `cmd_fuse_macos.rs`)
2. It unconditionally imports `spfs_vfs::proto` module (line 13 of `cmd_fuse_macos.rs`)
3. These modules are conditionally compiled and only exist when:
   - `target_os = "macos"` AND
   - `feature = "macfuse-backend"`

**Build Error on Linux**:
```
error[E0432]: unresolved import `spfs_vfs::macos`
  --> crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs:12:15
   |
12 | use spfs_vfs::macos::{get_parent_pid, Config, Service};
   |               ^^^^^ could not find `macos` in `spfs_vfs`
```

### Key Discoveries:

- **Existing Pattern**: `spfs-cli-winfsp` already handles this correctly using `#[cfg(windows)]` guards throughout `cmd_winfsp.rs` (lines 6-20)
- **No macOS CI**: GitHub Actions runs Linux and Windows builds, but not macOS, which is why this wasn't caught
- **Module Gates**: `spfs-vfs/src/lib.rs` correctly gates the `macos` module at lines 24-25 and `proto` module at lines 20-21

## Desired End State

After implementation:
1. The `spfs-cli-fuse-macos` binary compiles successfully on macOS
2. The binary provides clear compile-time errors when attempting to build on non-macOS platforms
3. The implementation matches the established Windows pattern in `spfs-cli-winfsp`
4. Linux CI builds pass without requiring `--exclude spfs-cli-fuse-macos`

**Verification**:
- `cargo build --workspace` succeeds on Linux (excludes macOS binary automatically)
- `cargo build --workspace` succeeds on macOS (includes macOS binary)
- `cargo build -p spfs-cli-fuse-macos` on Linux produces clear error message
- All existing tests continue to pass

## What We're NOT Doing

- Adding macOS builds to CI/CD (future work)
- Changing the Cargo.toml workspace structure
- Adding new feature flags
- Modifying the `spfs-vfs` library's conditional compilation
- Creating a build script to exclude platform-specific binaries

## Implementation Approach

Follow the exact pattern used by `spfs-cli-winfsp/src/cmd_winfsp.rs`, which uses `#[cfg(windows)]` guards to conditionally compile Windows-specific code and imports. We'll apply the same pattern using `#[cfg(target_os = "macos")]` for the macOS binary.

This approach:
- Is consistent with existing codebase patterns
- Provides clear compile-time errors
- Requires no Cargo.toml changes
- Is immediately understandable to developers

---

## Phase 1: Add Conditional Compilation Guards

### Overview
Add `#[cfg(target_os = "macos")]` attributes to all macOS-specific imports, types, and code blocks in the binary, following the Windows pattern.

### Changes Required:

#### 1. Guard Platform-Specific Imports
**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

**Lines 12-13**: Add `#[cfg(target_os = "macos")]` before the imports

```rust
#[cfg(target_os = "macos")]
use spfs_vfs::macos::{get_parent_pid, Config, Service};
#[cfg(target_os = "macos")]
use spfs_vfs::proto;
```

**Rationale**: These modules only exist on macOS builds, so imports must be conditional. This matches `cmd_winfsp.rs:15-20`.

#### 2. Guard Platform-Specific Functions and Types  
**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

**Lines 60-73**: Add `#[cfg(target_os = "macos")]` to the `run` method

```rust
#[cfg(target_os = "macos")]
impl CmdFuseMacos {
    fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .into_diagnostic()
            .wrap_err("Failed to establish async runtime")?;
        let res = match &mut self.command {
            Command::Mount(c) => rt.block_on(c.run(config)),
            Command::Service(c) => rt.block_on(c.run(config)),
        };
        rt.shutdown_timeout(std::time::Duration::from_secs(30));
        res
    }
}
```

**Lines 94-164**: Add `#[cfg(target_os = "macos")]` to `CmdService` impl

```rust
#[cfg(target_os = "macos")]
impl CmdService {
    async fn run(&mut self, config: &spfs::Config) -> Result<i32> {
        // ... existing implementation ...
    }

    async fn stop(&self) -> Result<i32> {
        // ... existing implementation ...
    }
}
```

**Lines 193-238**: Add `#[cfg(target_os = "macos")]` to `CmdMount` impl

```rust
#[cfg(target_os = "macos")]
impl CmdMount {
    async fn run(&mut self, _config: &spfs::Config) -> Result<i32> {
        // ... existing implementation ...
    }
}
```

**Lines 240-243**: Add `#[cfg(target_os = "macos")]` to helper function

```rust
#[cfg(target_os = "macos")]
fn is_connection_refused(err: &impl std::error::Error) -> bool {
    let err_str = err.to_string();
    err_str.contains("Connection refused") || err_str.contains("connection refused")
}
```

**Rationale**: All implementation code uses macOS-specific modules and should only compile on macOS.

#### 3. Update Main Function
**File**: `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`

**Lines 16-32**: Add `#[cfg(target_os = "macos")]` to main function and add non-macOS error path

```rust
#[cfg(target_os = "macos")]
pub fn main() -> Result<i32> {
    let mut opt = CmdFuseMacos::parse();
    opt.logging.syslog = true;
    // SAFETY: We're in a single-threaded context at this point
    unsafe { opt.logging.configure(); }

    let config = match spfs::get_config() {
        Err(err) => {
            tracing::error!(err = ?err, "failed to load config");
            return Ok(1);
        }
        Ok(config) => config,
    };

    let result = opt.run(&config);
    spfs_cli_common::handle_result!(result)
}

#[cfg(not(target_os = "macos"))]
pub fn main() -> Result<i32> {
    eprintln!("spfs-fuse-macos can only be built and run on macOS");
    eprintln!("For Linux, use: spfs-fuse");
    eprintln!("For Windows, use: spfs-winfsp");
    std::process::exit(1);
}
```

**Rationale**: Provides clear error message on non-macOS platforms instead of compilation failure. This matches the pattern in `cmd_winfsp.rs:25-42`.

#### 4. Add Documentation Comment
**File**: `crates/spfs-cli/cmd-fuse-macos/Cargo.toml`

**Lines 1-6**: Add a comment documenting platform requirement

```toml
[package]
name = "spfs-cli-fuse-macos"
version.workspace = true
authors.workspace = true
edition.workspace = true
license-file.workspace = true

# Note: This binary is macOS-specific and will not build on other platforms.
# Platform-specific code is gated with #[cfg(target_os = "macos")].
# For Linux, see spfs-cli-fuse. For Windows, see spfs-cli-winfsp.

[[bin]]
name = "spfs-fuse-macos"
path = "src/main.rs"
```

**Rationale**: Documents the platform requirement for future maintainers.

### Success Criteria:

#### Automated Verification:
- [x] Build on Linux succeeds: `cargo build --workspace`
- [x] Build on Linux succeeds with explicit check: `cargo check -p spfs-cli-fuse-macos`
- [x] Build on macOS succeeds (when available): `cargo build --workspace`
- [ ] Linting passes: `cargo clippy --workspace`
- [ ] All existing tests pass: `cargo test --workspace`
- [ ] CI Linux builds pass (GitHub Actions `build-and-test` job)
- [ ] CI Windows builds pass (GitHub Actions `build-windows` job)

#### Manual Verification:
- [ ] Building the binary directly on Linux with `cargo build -p spfs-cli-fuse-macos` produces a clear error message about macOS requirement
- [ ] The binary compiles and runs successfully on a macOS system
- [ ] Error messages clearly indicate which platforms are supported
- [ ] The pattern matches the Windows binary implementation style

---

## Testing Strategy

### Unit Tests:
No new unit tests required - existing tests should continue to pass. The conditional compilation is verified at build time.

### Integration Tests:
No integration test changes required. The SPFS integration tests already run platform-specific code appropriately.

### Manual Testing Steps:

1. **On Linux VM**:
   ```bash
   # Should succeed (excludes macOS binary)
   cargo build --workspace
   
   # Should succeed with warning about cfg-gated code
   cargo check -p spfs-cli-fuse-macos
   
   # Should show clear error message
   cargo run -p spfs-cli-fuse-macos -- --help
   ```

2. **On macOS (when available)**:
   ```bash
   # Should succeed and include macOS binary
   cargo build --workspace
   
   # Should run successfully
   cargo run -p spfs-cli-fuse-macos -- --help
   
   # Should show macOS-specific options
   ./target/debug/spfs-fuse-macos service --help
   ```

3. **Verify CI**:
   - Push branch and verify Linux CI builds pass
   - Verify Windows CI builds pass
   - Check that no new warnings are introduced

### Edge Cases to Test:
- Building with `--all-targets` should work on all platforms
- Building with specific features should still respect platform gates
- Clippy should not warn about unused imports or dead code
- Documentation builds should succeed: `cargo doc --workspace`

## Performance Considerations

No performance impact - this is purely compile-time conditional compilation. The runtime behavior on macOS is unchanged.

## Migration Notes

No migration required. This is a build-time fix that:
- Doesn't change any runtime behavior on macOS
- Prevents compilation errors on non-macOS platforms
- Requires no changes to deployment or usage

Developers on Linux/Windows will see clearer error messages if they accidentally try to build the macOS binary.

## References

- Original research: `.llm/shared/research/2025-12-01-linux-macos-build-errors.md`
- Windows pattern: `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs` (lines 6-20)
- VFS library gates: `crates/spfs-vfs/src/lib.rs` (lines 20-25)
- CI configuration: `.github/workflows/rust.yml`
