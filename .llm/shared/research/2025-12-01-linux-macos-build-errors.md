---
date: 2025-12-01T10:56:26+0000
researcher: Andrew Paxson
git_commit: 0da1e12aba78d9d1769f4ee0bf6aca903d5b5997
branch: feature/macos-fuse-auto-start
repository: spk
topic: "Linux build errors with macOS-specific code that don't occur on macOS"
tags: [research, codebase, build-errors, cross-platform, conditional-compilation, spfs-vfs]
status: complete
last_updated: 2025-12-01
last_updated_by: Andrew Paxson
---

# Research: Linux build errors with macOS-specific code that don't occur on macOS

**Date**: 2025-12-01T10:56:26+0000  
**Researcher**: Andrew Paxson  
**Git Commit**: 0da1e12aba78d9d1769f4ee0bf6aca903d5b5997  
**Branch**: feature/macos-fuse-auto-start  
**Repository**: spk

## Research Question

Why are there Linux build errors in the spk repository that don't occur on macOS builds?

## Summary

The build errors on Linux are caused by **macOS-specific binaries attempting to compile on Linux**. The `spfs-cli-fuse-macos` binary unconditionally tries to import modules from `spfs-vfs` that are gated behind `#[cfg(target_os = "macos")]` and feature flags. When building on Linux, these modules don't exist, causing compilation failures.

This is a **conditional compilation issue** where platform-specific binaries are not properly excluded from the build on non-target platforms.

## Detailed Findings

### Build Error Context

**Linux Build Errors** (2 compilation errors in `spfs-cli-fuse-macos`):
```
error[E0432]: unresolved import `spfs_vfs::macos`
  --> crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs:12:15
   |
12 | use spfs_vfs::macos::{get_parent_pid, Config, Service};
   |               ^^^^^ could not find `macos` in `spfs_vfs`
   |
note: found an item that was configured out
  --> /work/projects/spk/crates/spfs-vfs/src/lib.rs:25:9
   |
25 | pub mod macos;
   |         ^^^^^
note: the item is gated here
  --> /work/projects/spk/crates/spfs-vfs/src/lib.rs:24:1
   |
24 | #[cfg(all(target_os = "macos", feature = "macfuse-backend"))]
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

error[E0432]: unresolved import `spfs_vfs::proto`
  --> crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs:13:5
   |
13 | use spfs_vfs::proto;
   |     ^^^^^^^^^^^^^^^ no `proto` in the root
   |
note: found an item that was configured out
  --> /work/projects/spk/crates/spfs-vfs/src/lib.rs:21:9
   |
21 | pub mod proto;
   |         ^^^^^
note: the item is gated here
  --> /work/projects/spk/crates/spfs-vfs/src/lib.rs:20:1
   |
20 | #[cfg(all(any(target_os = "macos", windows), any(feature = "macfuse-backend", feature = "winfsp-backend")))]
   | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**macOS Build**: Succeeds because the conditional compilation gates pass

### Root Cause: Missing Platform-Specific Build Configuration

The `spfs-cli-fuse-macos` binary is being compiled on Linux even though:
1. Its dependencies (`spfs_vfs::macos` and `spfs_vfs::proto`) are conditionally compiled
2. These modules only exist when `target_os = "macos"` **AND** the `macfuse-backend` feature is enabled
3. The binary itself has no conditional compilation to exclude it from Linux builds

### Conditional Compilation Analysis

**`spfs-vfs/src/lib.rs` module gates:**
```rust
// Line 20-21: proto module
#[cfg(all(any(target_os = "macos", windows), any(feature = "macfuse-backend", feature = "winfsp-backend")))]
pub mod proto;

// Line 24-25: macos module  
#[cfg(all(target_os = "macos", feature = "macfuse-backend"))]
pub mod macos;
```

**`spfs-cli-fuse-macos/src/cmd_fuse_macos.rs` imports:**
```rust
// Line 12: Imports macos module unconditionally
use spfs_vfs::macos::{get_parent_pid, Config, Service};

// Line 13: Imports proto module unconditionally
use spfs_vfs::proto;
```

The binary attempts to use these modules without checking if they're available on the target platform.

### Cargo.toml Analysis

### Proposed Solutions

There are several ways to fix this issue:

**Option 1: Exclude the binary from non-macOS builds in Cargo.toml**
```toml
[[bin]]
name = "spfs-fuse-macos"
path = "src/main.rs"
required-features = ["macos-build"]  # Add a feature flag
```

Then gate the entire binary with conditional compilation in the source.

**Option 2: Add conditional compilation to the binary's main.rs**
```rust
#[cfg(not(target_os = "macos"))]
compile_error!("spfs-fuse-macos can only be built on macOS");
```

**Option 3: Exclude from workspace members on Linux**
Use cargo's `--exclude` flag when building on Linux:
```bash
cargo build --workspace --exclude spfs-cli-fuse-macos
```

**Option 4: Add platform-specific workspace members**
Restructure the workspace to conditionally include macOS-specific crates only when building on macOS.

## Code References

- `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs:12` - Unconditional import of macos module
- `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs:13` - Unconditional import of proto module
- `crates/spfs-vfs/src/lib.rs:20-21` - Conditional compilation of proto module
- `crates/spfs-vfs/src/lib.rs:24-25` - Conditional compilation of macos module
- `crates/spfs-cli/cmd-fuse-macos/Cargo.toml:8-10` - Binary definition without platform guards
- `Cargo.toml:5` - Workspace members including `crates/spfs-cli/*`

## Architecture Insights

### Workspace Structure

The project uses a Cargo workspace with glob patterns for members:
- `crates/spfs-cli/*` includes ALL binaries in that directory
- No mechanism to exclude platform-specific binaries from the build
- Both `spfs-cli-fuse` (Linux) and `spfs-cli-fuse-macos` (macOS) are included unconditionally

### Cross-Platform VFS Backends

The `spfs-vfs` crate provides multiple VFS backends:
- **Linux**: FUSE backend via `fuser` crate
- **macOS**: macFUSE backend via `fuser` with `macfuse-4-compat` feature and `target_os = "macos"` gate
- **Windows**: WinFSP backend (gated similarly)

Each backend has its own CLI binary:
- `spfs-fuse` - Linux FUSE
- `spfs-fuse-macos` - macOS macFUSE  
- `spfs-winfsp` - Windows WinFSP

### Conditional Compilation Pattern

The `spfs-vfs` library correctly uses conditional compilation:
```rust
#[cfg(all(target_os = "macos", feature = "macfuse-backend"))]
pub mod macos;
```

However, the CLI binaries don't have matching guards, causing them to attempt compilation on all platforms.

## Immediate Workaround

To build on Linux without errors, exclude the macOS-specific binary:

```bash
cargo build --workspace --exclude spfs-cli-fuse-macos
```

Or when using limactl:
```bash
limactl shell --shell /bin/zsh --workdir /work/projects/spk fedora \
  cargo build --workspace --exclude spfs-cli-fuse-macos
```

## Recommended Fix

Add a compile-time error to the macOS binary to make the platform requirement explicit:

**`crates/spfs-cli/cmd-fuse-macos/src/main.rs`:**
```rust
#![cfg(target_os = "macos")]
// Or alternatively:
#[cfg(not(target_os = "macos"))]
compile_error!("spfs-fuse-macos can only be built on macOS");
```

This will provide a clear error message and prevent accidental compilation on other platforms.

## Open Questions

1. Should platform-specific CLI binaries be excluded from the workspace on non-matching platforms?
2. Is there a CI/CD pipeline that tests builds on both Linux and macOS?
3. Should the workspace use `default-members` to exclude platform-specific binaries?
4. Are there other platform-specific binaries that might have similar issues?

## Related Files

- `crates/spfs-cli/cmd-fuse/` - Linux FUSE CLI (should work on Linux)
- `crates/spfs-cli/cmd-fuse-macos/` - macOS FUSE CLI (fails on Linux)
- `crates/spfs-cli/cmd-winfsp/` - Windows WinFSP CLI (likely fails on Linux/macOS)
- `crates/spfs-vfs/src/fuse_linux.rs` - Linux FUSE implementation
- `crates/spfs-vfs/src/macos/` - macOS macFUSE implementation
