---
date: 2025-11-29T10:30:00-08:00
repository: spk
git_commit: 263ab61c feat (spfs): extend prune for zombie'd runtimes
branch: feature/macos-fuse-auto-start
discovery_prompt: "does @.llm/shared/context/2025-11-28-spk-spfs-fuse.md need updated?"
generated_by: "opencode:/discovery"
tags: [context, discovery]
status: complete
last_updated: 2025-12-07
---

# Repo Context Guide: spk (SpFS Runtime + FUSE Integration)

## TL;DR
- SpFS supports two Linux FUSE modes: OverlayFs-with-FUSE (hybrid) and FuseOnly (pure VFS) selected via `runtime::MountBackend` (`crates/spfs/src/runtime/storage.rs`). On macOS, macFUSE provides `FuseWithScratch` backend for editable mounts with copy-on-write scratch directories.
- `spfs-enter` orchestrates mount namespaces; when FUSE is enabled it launches the dedicated `spfs-fuse` binary to expose manifests as a live filesystem before overlaying edits (`crates/spfs/src/env.rs`, `status_unix.rs`). On macOS, `env_macos.rs` uses `spfs-fuse-macos` service with PID-based router.
- The `spfs-fuse` daemon (from `crates/spfs-cli/cmd-fuse`) wraps `spfs-vfs` (FUSE filesystem impl) plus `fuser::Session` to serve stack manifests from local/remote repos (`crates/spfs-vfs/src/fuse.rs`). macOS uses `crates/spfs-cli/cmd-fuse-macos` with gRPC service and router (`crates/spfs-vfs/src/macos/router.rs`).
- Runtime monitor (`spfs-monitor`) heartbeats the FUSE mount so it can self-terminate if clients disappear, avoiding zombie mounts (`crates/spfs/src/monitor.rs`). macOS uses service auto-start (`ensure_service_running`) and PID-based cleanup.
- Configuration for worker threads, heartbeats, mount options, secondary repositories lives under `[filesystem]` and `[fuse]` in `spfs` config (`docs/admin/config.md`, `crates/spfs/src/config.rs`). macOS adds `mount_options` for macFUSE and scratch directory settings.
- Cleanup involves `spfs-enter --exit` unmounting overlay first, then FUSE via `fusermount -u` retries to prevent stale mounts (`crates/spfs/src/env.rs`). On macOS, `umount` and router unmount via gRPC.

## Quickstart (dev)

### Linux
- **Prereqs**: Linux with FUSE3 (or FUSE2 + `fusermount`), `CAP_SYS_ADMIN` on `spfs-enter` and `spfs-fuse` binaries (`Makefile.linux` target). `fuser` crate built via workspace features `fuse-backend-*`.
- **Setup**:
  1. Enable fuse backend in config (`~/.config/spfs/config.toml`: `filesystem.backend = "OverlayFsWithFuse"` or `"FuseOnly"`).
  2. Build binaries: `cargo build -p spfs-cli -p spfs-cli-fuse`.
  3. Grant caps: `sudo setcap 'cap_sys_admin+ep' target/debug/spfs-enter target/debug/spfs-fuse`.
- **Run**:
  - Hybrid runtime: `spfs run TAG --filesystem-backend overlayfs-with-fuse -- bash` (CLI flag resolves to config override) → overlay upper/work + FUSE lower.
  - Fuse-only: `spfs run TAG --filesystem-backend fuse -- bash` (requires writable mountpoint, default `/spfs`).
- **Test**:
  - `cargo test -p spfs --features fuse-backend -- runtime::mount_backend` (unit tests).
  - Integration: `crates/spfs/tests/integration/unprivileged/test_fuse_du.sh` (ensures `du` works) & `test_fuse_cleanup.sh` (process cleanup) – run via `make test-fuse` (see script header for env requirements).

### macOS
- **Prereqs**: macOS with macFUSE installed (`brew install --cask macfuse`). The `/spfs` mount point must exist (configure via `Makefile.macos setup-spfs-mount`). `fuser` crate with `macfuse-4-compat` feature.
- **Setup**:
  1. Enable macFUSE backend in config (`~/.config/spfs/config.toml`: `filesystem.backend = "FuseWithScratch"` for editable mounts, `"FuseOnly"` for read-only).
  2. Build binaries: `cargo build -p spfs-cli -p spfs-cli-fuse-macos`.
  3. Ensure `/spfs` directory exists and is writable (may require synthetic.conf setup and reboot).
- **Run**:
  - Editable runtime: `spfs run TAG --filesystem-backend fuse-with-scratch -- bash` (uses copy-on-write scratch directory).
  - Read-only Fuse-only: `spfs run TAG --filesystem-backend fuse -- bash` (single macFUSE mount with PID-based routing).
- **Test**:
  - `cargo test -p spfs-vfs --features macos-fuse` (unit tests for macOS modules).
  - Integration: `crates/spfs/tests/integration/unprivileged/test_fuse_du.sh` (also works on macOS with macFUSE).
- **Service management**: The macFUSE service auto-starts via `ensure_service_running`. Manual control: `spfs-fuse-macos service` (start/stop).

### Common commands (cross-platform)
- `spfs run <tag> --filesystem-backend overlayfs-with-fuse -- cmd` (Linux)
- `spfs shell <tag> --fuse-only` (Linux/macOS read-only)
- `spfs shell <tag> --edit` (macOS editable via `FuseWithScratch`)
- `spfs runtime info <name>` (inspect backend field)
- `spfs runtime remount <name>` (reinitializes FUSE lower dir)

## How to use (user)

### Linux
- **Hybrid overlay**: Use overlay for edits but source immutable stack via FUSE to avoid pre-rendering. Example `spfs run plat/base+tools -- filesystem-backend overlayfs-with-fuse -- make`. FUSE exposes stack at runtime-configured `lower_dir`; overlay upper/work capture mutations.
- **Read-only FuseOnly**: `spfs run base --filesystem-backend fuse-only -- cat /spfs/...` yields direct FUSE mount; no overlay upper, so runtime is read-only and mask files not applied.
- **Remote-through reads**: Provide remote hints via config `[filesystem.secondary_repositories]` or CLI `--filesystem-remote origin` so FUSE can fetch objects on demand (`spfs_vfs::Config::remotes`).
- **Heartbeat-sensitive workflows**: If monitor dies (e.g., host reboot), FUSE self-shutdown after heartbeat grace period; re-run `spfs run --rerun <runtime>` to rehydrate environment.

### macOS
- **Editable mounts with copy-on-write**: Use `--filesystem-backend fuse-with-scratch` (or `--edit` flag) for editable runtimes. Example `spfs shell --edit my-package/1.0.0`. Writes go to scratch directory; reads check scratch first, then base repository.
- **Read-only FuseOnly**: Same as Linux but uses PID-based router; multiple runtimes share single macFUSE mount.
- **Service auto-start**: The macFUSE service starts automatically on first mount; manual control via `spfs-fuse-macos service` (start/stop).
- **Commit workflow**: After making changes in an editable shell, use `spfs commit` to persist modifications back to repository; scratch directory changes are captured.
- **Process tree isolation**: Each shell session gets its own filesystem view based on its process tree root PID; child processes inherit the same view.

## Repo map

### Core (cross-platform)
- `crates/spfs/src/runtime/storage.rs`: runtime config/backends + serialization of `MountBackend` (`OverlayFsWithFuse`, `FuseOnly`, `FuseWithScratch`) and lower/upper dirs.
- `crates/spfs/src/config.rs` & `docs/admin/config.md`: user-facing config knobs for `[filesystem]` `[fuse]`.
- `crates/spfs/src/monitor.rs`: heartbeat + PID tracking for FUSE runtimes.

### Linux-specific
- `crates/spfs/src/env.rs`: namespace + mount operations; launching/unmounting `spfs-fuse`.
- `crates/spfs/src/status_unix.rs`: high-level init/reinit/durable flows, toggling overlay vs FUSE.
- `crates/spfs-vfs/src/fuse.rs`: actual FUSE filesystem implementing `fuser::Filesystem` over manifests/repositories.
- `crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs`: CLI entrypoint/daemonization, option parsing, heartbeat wiring.
- Tests: `crates/spfs/tests/integration/unprivileged/test_fuse_*`.

### macOS-specific
- `crates/spfs/src/env_macos.rs`: macOS implementation of environment management, uses `spfs-fuse-macos` service with PID-based router.
- `crates/spfs/src/status_macos.rs`: macOS-specific initialization and remount flows.
- `crates/spfs/src/process_macos.rs`: macOS process tracking via `libproc`.
- `crates/spfs/src/monitor_macos.rs`: macOS-specific monitor logic.
- `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs`: CLI for macFUSE service, mount, unmount, status commands.
- `crates/spfs-vfs/src/macos/`: macOS FUSE implementation modules:
  - `mod.rs`, `config.rs`, `service.rs`, `router.rs`, `mount.rs`, `handle.rs`, `scratch.rs`, `process.rs`
- `Makefile.macos`: macOS-specific build targets and setup.

### Documentation
- `docs/spfs/develop/design.md` (process graph) & `docs/spfs/develop/runtime.md` (runtime layering overview).
- `docs/spfs/develop/macos-fuse-architecture.md`: comprehensive macOS architecture details.
- `docs/spfs/macos-getting-started.md`: macOS user guide.

## Architecture overview

### Linux FUSE Architecture
- **Mount backend selection**: `runtime::Config.mount_backend` sets `OverlayFsWithFuse` or `FuseOnly`. CLI resolves this via config/flags before writing runtime data to repository (`runtime/storage.rs`).
- **Namespace orchestration**: `env::RuntimeConfigurator` enters mount namespace, becomes root, ensures directories. For FUSE modes, overlay path differs:
  - Hybrid: `mount_runtime` prepares tmpfs upper/work, then `mount_fuse_lower_dir` spawns `spfs-fuse` into background at `lower_dir`, overlay mounts `/spfs` with lower stack `[lower_dir, rendered layers]`.
  - Fuse-only: Skips overlay; `mount_env_fuse` launches `spfs-fuse` targeting `/spfs` directly and then binds live layers.
- **FUSE daemon**: `spfs-fuse` uses `spfs-vfs::Session` to:
  - Resolve EnvSpec into manifest (via `spfs::tracking`), allocate inodes, keep TTL infinity (readonly).
  - Serve `fuser` callbacks (lookup/read/open/readdir) by hitting `RepositoryHandle`s for payloads (local FS + optional remotes). Maintains handle/inode caches via `DashMap`.
  - Manage mount options (allow_other, remotes, uid/gid) and uses `tokio` runtime for async IO & signal handling.
- **Monitor heartbeat**: `spfs-monitor` tracks target PIDs via procfs; when runtime uses FUSE it sends periodic stat requests to `/spfs/.spfs-heartbeat-<uuid>-<ulid>` to keep `spfs-fuse` alive. If heartbeats stop, `spfs-fuse` gracefully shuts down to avoid dangling mounts (see `monitor.rs` + `cmd_fuse.rs`).
- **Cleanup flow**: `spfs-enter --exit` (triggered by monitor) calls `unmount_env` which unmounts FUSE first using `fusermount -u|-uz` with retries, then overlay. Fuse-only also unmounts live-layer bind mounts prior to `fusermount`.

### macOS macFUSE Architecture
- **Mount backend selection**: macOS adds `FuseWithScratch` backend for editable mounts with copy-on-write scratch directories. `FuseOnly` also available for read-only mounts. Default on macOS when editable mode requested is `FuseWithScratch`. Backend selection occurs in `runtime::Config.mount_backend` (cross‑platform) and influences scratch directory creation.
- **Service orchestration**: Instead of Linux mount namespaces, macOS uses a persistent gRPC service (`spfs-fuse-macos service`) that auto‑starts via `ensure_service_running`. This function checks if the service is listening on `127.0.0.1:37738`; if not, it spawns the service daemon and waits for readiness with exponential backoff. The service runs a single macFUSE mount at `/spfs` that serves all processes.
- **PID‑based router**: The single macFUSE mount delegates requests to per‑runtime `Mount` instances via `crates/spfs‑vfs/src/macos/router.rs`. The router determines which mount to use by walking the caller’s process ancestry using `libproc` (`get_parent_pids_macos`). Each runtime registers its root PID with the router via the gRPC `mount` command; child processes inherit the same mount. A default empty mount handles requests from unknown PIDs.
- **Editable mounts with copy‑on‑write**: When `FuseWithScratch` is selected, the mount creates a scratch directory under `~/Library/Caches/spfs/scratch/{runtime_name}` (managed by `ScratchDir`). Write operations (`create`, `write`, `unlink`, `mkdir`, `rmdir`, `rename`) target the scratch directory. Reads check scratch first, then fall back to the base repository. Copy‑up on write (`perform_copy_up`) copies the blob from the repository to scratch when a file is opened with write flags, preserving permissions.
- **Whiteout deletions**: Deleted files are tracked via a `HashSet` of virtual paths in the scratch directory; `lookup` returns `ENOENT` if a whiteout exists for the path.
- **Process lifecycle tracking**: A `ProcessWatcher` monitors root PID exits via `EVFILT_PROC` kqueue events. On exit, the router removes the mount and cleans up the scratch directory. A periodic garbage collection task (every 5 seconds) also scans for dead processes and orphaned mounts.
- **Heartbeat & monitoring**: macOS does not use the same heartbeat file mechanism as Linux; instead, the `spfs‑monitor‑macos` tracks runtime processes via `libproc` and sends periodic gRPC keep‑alive pings. If the monitor stops, the service will eventually garbage‑collect orphaned mounts after a grace period.
- **Cleanup flow**: Unmount is triggered via gRPC `unmount` command (called by `spfs‑enter‑‑exit` or monitor), which removes the route and cleans up the scratch directory. When no active mounts remain, the service unmounts the global `/spfs` mount via `umount`. Abnormal termination leaves scratch directories; orphan cleanup runs on service start.

## Key components (deep links)

### `runtime::MountBackend` (`crates/spfs/src/runtime/storage.rs`)
- **Purpose**: Encodes mount mode for runtime (overlay renders vs overlay+FUSE vs FUSE-only vs WinFsp).
- **Entry points**: `Config::is_backend_fuse`, `MountBackend::is_fuse`, `requires_localization` (decides whether layers must render before run).
- **Invariants**: FUSE backends skip render_localization (lazy fetch) but require `lower_dir` rotation between remounts to avoid lazy unmount conflicts.

### `env::RuntimeConfigurator::mount_fuse_*` (`crates/spfs/src/env.rs`)
- **Purpose**: Launch `spfs-fuse` with correct args, block until mount is ready, and tie lifecycle to runtime namespace.
- **Flow**: Spawns thread to run `spfs-fuse -o <opts> <platform-digest> <mount-path>`; waits for mountpoint to exist; asynchronous join ensures errors bubble up. Unmount functions use `fusermount` with exponential backoff.
- **Important invariants**: Must run inside mount namespace and as root to allow `allow_other`, `auto_unmount`. Logging is suppressed to avoid blocking on pipes.

### `spfs-vfs::Session` (`crates/spfs-vfs/src/fuse.rs`)
- **Purpose**: Implementation of `fuser::Filesystem` over spfs manifests.
- **Key types**: `Config` (uid/gid, mount options, remote names), `Filesystem` (inodes/handles), `Handle` enum (local file, remote stream, directory iterator).
- **Behavior**: On `open`, looks for payload in local FS repo; fallback to remote via `RepositoryHandle::open_payload`. Directory reads stream entries; `statfs` returns synthetic values. Maintains TTL to avoid timestamp churn.
- **Invariants**: runtime is read-only; mask entries return `ENOENT`; ensures root attr is `S_IFDIR` even if manifest missing bits.

### `spfs-cli/cmd-fuse` (`crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs`)
- **Purpose**: CLI binary invoked by runtime to run FUSE FS.
- **Entry**: `CmdFuse::run` builds config, enforces mountpoint permissions, handles daemonization (unless `--foreground`) and multi-threaded tokio runtime; registers signal handlers to unmount gracefully.
- **Custom options**: `--options remote=<name>,uid=...,gid=...`; `allow_other` etc forwarded to `fuser`.
- **Heartbeat**: When enabled, spawns tokio task checking `Session::seconds_since_last_heartbeat()` and triggers unmount if monitor stops calling.

### `spfs::monitor` heartbeat (`crates/spfs/src/monitor.rs`)
- **Purpose**: Keep runtimes tidy, drive heartbeat for FUSE to ensure monitor liveliness.
- **Flow**: Watches cgroup of runtime owner PIDs; when empty, repairs via mount namespace scan; ticks every `fuse.heartbeat_interval_seconds` to touch random heartbeat file under `/spfs/`.

### macOS router (`crates/spfs-vfs/src/macos/router.rs`)
- **Purpose**: PID-based filesystem router for macOS; delegates fuser requests to per-process `Mount` instances by walking caller's process tree via `libproc`.
- **Key types**: `Router` implements `fuser::Filesystem`; maintains `routes: DashMap<u32, Arc<Mount>>`; `default` empty mount for unknown PIDs.
- **Behavior**: `get_mount_for_pid` walks ancestry using `get_parent_pids_macos`; returns first registered mount or default. `mount`/`unmount` register/unregister root PIDs.
- **Process lifecycle**: `start_cleanup_task` watches for process exits via `ProcessWatcher`; cleans up orphaned mounts and scratch directories.

### `FuseWithScratch` mount backend (`crates/spfs-vfs/src/macos/mount.rs`)
- **Purpose**: Per-runtime filesystem mount supporting copy-on-write editable operations via scratch directory.
- **Key types**: `Mount` manages inodes (`DashMap<u64, Arc<Entry<u64>>>`), handles (`DashMap<u64, Handle>`), scratch directory (`Option<ScratchDir>`).
- **Editable semantics**: `is_editable()` true when scratch present. Write operations (`create`, `write`, `unlink`, `mkdir`, `rmdir`, `rename`, `setattr`) operate on scratch.
- **Copy-up on write**: `perform_copy_up` copies blob from repository to scratch when file opened with write flags; allocates new inode and registers path mappings.
- **Whiteout handling**: `lookup` checks `scratch.is_deleted`; `unlink` marks deleted via `scratch.mark_deleted`.

### macOS service auto-start (`ensure_service_running` in `crates/spfs/src/env_macos.rs`)
- **Purpose**: Automatically start macFUSE service daemon on first mount attempt; manage service lifecycle.
- **Flow**: Checks if gRPC service is listening at `127.0.0.1:37738`; if not, spawns `spfs-fuse-macos service` in background; waits for readiness with exponential backoff.
- **Retry logic**: Up to `MAX_SERVICE_START_RETRIES` attempts; provides user-friendly error if macFUSE not installed.
- **Integration**: Called by `env_macos.rs` `mount_env_fuse` and `RootConfigurator::mount_env_fuse`.

### Scratch directory (`crates/spfs-vfs/src/macos/scratch.rs`)
- **Purpose**: Manage copy-on-write scratch directory for editable mounts; provides whiteout tracking.
- **Location**: `~/Library/Caches/spfs/scratch/{runtime_name}/` (macOS cache directory).
- **Operations**: `copy_to_scratch`, `create_file`, `create_dir`, `mark_deleted`, `rename`, `cleanup`.
- **Whiteout tracking**: `HashSet<PathBuf>` of deleted virtual paths; `is_deleted` checks.
- **Automatic cleanup**: `Drop` implementation removes scratch directory; `cleanup_orphaned_scratch_directories` removes stale directories older than 24 hours.

## Configuration & environments

### Cross-platform configuration
- **Config file**: `~/.config/spfs/config.toml` (or `$SPFS_CONFIG`).
  - `[filesystem] backend = "OverlayFsWithFuse" | "FuseOnly"` toggles FUSE usage (Linux). On macOS, also `"FuseWithScratch"` for editable mounts.
  - `secondary_repositories = [...]` used to seed `spfs-vfs` remote search order (both Linux and macOS).
  - `use_mount_syscalls` irrelevant for FuseOnly but still affects overlay stage when hybrid (Linux only).
- **FUSE-specific** `[fuse]` keys: `worker_threads`, `max_blocking_threads` (tokio runtime sizing inside `spfs-fuse`); `enable_heartbeat`, `heartbeat_interval_seconds`, `heartbeat_grace_period_seconds` control monitor interplay (Linux). macOS uses separate service with its own threading.
- **Env vars**: `SPFS_FUSE_LOG_FOREGROUND=1` keeps logs in stderr; `SPFS_FILESYSTEM_TMPFS_SIZE` still honored for overlay upper/work even in FUSE hybrid (Linux); `SPFS_RUNTIME` identifies active runtime for monitor/resume.

### macOS-specific configuration
- **macOS mount options**: Configured via `crates/spfs-vfs/src/macos/config.rs` `Config` struct; includes `mountpoint` (default `/spfs`), `remotes`, `mount_options` (e.g., `allow_other`, `nodev`, `noatime`).
- **Service auto-start**: Controlled by `ensure_service_running` with environment variable `SPFS_MACFUSE_LISTEN_ADDRESS` (default `127.0.0.1:37738`).
- **Scratch directory location**: Default `~/Library/Caches/spfs/scratch/`; can be overridden via `ScratchDir::new` with runtime name.
- **Binary resolution**: macOS uses `which_spfs("fuse-macos")` to locate `spfs-fuse-macos`. Build with `cargo build -p spfs-cli-fuse-macos`.
- **macOS-specific env vars**: `SPFS_MACFUSE_LISTEN_ADDRESS`, `SPFS_FUSE_MACOS_LOG_FILE`.

## Testing & quality

### Linux
- **Integration scripts** (`crates/spfs/tests/integration/unprivileged`): verify FUSE mount cleanup/resilience (`test_fuse_cleanup.sh`, `test_fuse_du.sh`).
- **Unit coverage**: `spfs-vfs/src/fuse.rs` includes no tests (relies on runtime tests), but CLI and runtime modules have targeted tests. Use `cargo test -p spfs-cli --features fuse-backend` for CLI-level coverage.
- **CI**: Repo cspell + lint workflows but FUSE tests typically gated due to privilege requirements; run manually on Linux hosts.

### macOS
- **Unit tests**: `crates/spfs-vfs/src/macos/` modules have unit tests (e.g., `router.rs`, `scratch.rs`, `mount.rs`). Run with `cargo test -p spfs-vfs --features macos-fuse`.
- **Integration**: Same Linux integration scripts work on macOS if macFUSE installed; test PID‑based routing and editable mount workflows.
- **CI**: macOS CI runs lint and unit tests; integration tests require macFUSE installation (manual).

## Extension points & customization
- **New remotes**: Extend `[filesystem.secondary_repositories]` and ensure relevant `RepositoryHandle`s exist; `spfs-vfs` will iterate in declared order.
- **Additional mount options**: Extend `parse_options_from_args` in `cmd-fuse.rs` to support new `MountOption` or custom key/value pairs.
- **Writable FUSE**: Currently unimplemented (open with `O_WRONLY` rejects). Adding support requires modifications in `Filesystem::open` and handle types to stage writes, plus overlay coordination.
- **Heartbeat hooks**: `spfs-monitor` `enable_heartbeat` gating; can integrate alternative liveness signals by extending `monitor.rs` tick logic if more domains need to notify `spfs-fuse`.

## Operational notes

### Linux-specific
- **Deployment**: `spfs-fuse` must run privileged (root or allow-other) to permit `allow_other` mounts; packaging sets capabilities via RPM spec `%caps(cap_sys_admin+ep)` on `/usr/local/bin/spfs-fuse` (`spfs.spec`).
- **Observability**: `spfs-fuse` logs to `/tmp/spfs-runtime/fuse.log` (default) + syslog; adjust via CLI `--log-file`. Heartbeat failures logged at WARN + Sentry event via `warn_and_sentry_event!`.
- **Performance**: `spfs-vfs` caches entire manifest tree in memory (inode map). For huge platforms, expect larger RSS; consider flattening/limiting stack. Worker threads default `min(num_cpus, 8)`; increase to improve concurrency at cost of CPU.
- **Cleanup pitfalls**: Lazy unmount used when remounting overlay because FUSE may still be busy. Monitor ensures `fusermount` gets multiple attempts; if `spfs-fuse` hung (monitor crash) there is fallback to heartbeat-induced shutdown.

### macOS-specific limitations
- **No overlayfs**: macOS lacks native overlayfs support; editable mounts use copy-on-write scratch directories (`FuseWithScratch`) with userspace COW semantics.
- **No mount namespaces**: macOS lacks mount namespace isolation; PID-based router provides filesystem view separation but does not prevent cross‑process mount visibility.
- **No durable runtimes**: macOS does not support durable runtimes (`--keep-runtime`); runtimes are tied to process tree lifetime via PID routing. Changes must be committed before process exit.
- **Scratch directory persistence**: Scratch directories (`~/Library/Caches/spfs/scratch/`) may persist after abnormal termination; orphan cleanup runs on service start.
- **macFUSE installation required**: Requires macFUSE kernel extension; on Apple Silicon may need Recovery Mode approval.
- **Single mount point**: All runtimes share the same `/spfs` mount point; isolation via PID routing adds slight overhead per filesystem operation.

## LLM working set

### Core (cross‑platform)
1. `crates/spfs/src/runtime/storage.rs` – defines `MountBackend` (`OverlayFsWithFuse`, `FuseOnly`, `FuseWithScratch`), runtime config, detection helpers (`is_backend_fuse`, `rotate_lower_dir`).
2. `crates/spfs/src/config.rs` & `docs/admin/config.md` – user‑tunable settings for `[filesystem]` and `[fuse]`.
3. `crates/spfs/src/monitor.rs` – heartbeat + PID tracking interplay.

### Linux‑specific
4. `crates/spfs/src/env.rs` – `mount_fuse_*`, `unmount_env_*`, namespace guards.
5. `crates/spfs/src/status_unix.rs` – initialization/remount flows with FUSE branches.
6. `crates/spfs-vfs/src/fuse.rs` – FUSE filesystem logic (lookup/read/open semantics, repo access).
7. `crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs` – daemon lifecycle, CLI options, heartbeat integration.
8. `crates/spfs/tests/integration/unprivileged/test_fuse_cleanup.sh` – expected cleanup behavior.
9. `spfs.spec` & `Makefile.linux` – packaging/capability setup for FUSE binaries.

### macOS‑specific
10. `crates/spfs/src/env_macos.rs` – macOS environment management, service auto‑start (`ensure_service_running`), mount/unmount via `spfs-fuse-macos`.
11. `crates/spfs/src/status_macos.rs` – macOS‑specific initialization and remount flows.
12. `crates/spfs/src/process_macos.rs` & `monitor_macos.rs` – process tracking and monitor logic.
13. `crates/spfs-cli/cmd-fuse-macos/src/cmd_fuse_macos.rs` – CLI for macFUSE service, mount, unmount, status.
14. `crates/spfs-vfs/src/macos/` – entire macOS FUSE implementation:
    - `router.rs` – PID‑based router implementing `fuser::Filesystem`.
    - `mount.rs` – `FuseWithScratch` mount with copy‑on‑write scratch.
    - `scratch.rs` – scratch directory management and whiteout tracking.
    - `service.rs` – gRPC service and lifecycle.
    - `config.rs`, `handle.rs`, `process.rs`.
15. `Makefile.macos` – macOS‑specific build targets and mount‑point setup.
16. `docs/spfs/develop/macos-fuse-architecture.md` – comprehensive macOS architecture details.

## Open questions

### General FUSE
- How does `spfs-fuse` authenticate/authorize access to secondary remotes? (Inspect repository credential handling in `spfs::storage`.)
- What retry/backoff exists when repo reads fail mid-request? (Search `Handle::read`/`read_dir` in `spfs-vfs/src/fuse.rs`.)
- Are there plans for write support via FUSE on Linux (overlayless edits)? (Track TODO in `cmd-fuse.rs` around `rw` option.)
- How does Windows WinFSP backend differ in capabilities vs FuseOnly? (Review `crates/spfs-cli/cmd-winfsp` + `runtime/winfsp.rs`.)
- What telemetry is emitted when heartbeat shutdown occurs? (Follow `warn_and_sentry_event!` definitions in `spfs_cli_common`.)

### macOS-specific
- **PID reuse**: The router's cleanup loop uses kqueue `EVFILT_PROC` to detect process exits and removes mounts immediately. Garbage collection scans for dead processes every 5 seconds. PID reuse is unlikely within this window, but the system also validates that the root PID is still alive before routing.
- **Performance impact of PID ancestry walking**: Each FUSE operation walks the ancestry chain via libproc (fallback to sysctl). This adds overhead but is acceptable for typical workloads. Future optimization could add a PID→mount cache with invalidation on exit.
- **Scratch directory permissions**: Scratch directories are created under `~/Library/Caches/spfs/scratch/` (user‑specific cache). Multi‑user isolation is provided by OS filesystem permissions. Root‑run services use root's home directory.
- **Editable mounts commit incremental**: The `spfs commit` command captures all changes in the scratch directory (modified and deleted files) and creates a new layer. Incremental commits are possible by committing only specific paths (future feature).
- **Service crash recovery**: On startup, the service cleans up orphaned scratch directories older than 24 hours. The router's garbage collection also removes mounts for dead processes. A crashed service loses in‑memory mount state; runtimes must be re‑entered.
- **Durable runtimes on macOS**: Not currently supported (`--keep‑runtime` is a no‑op). The macOS backend lacks mount namespace isolation, making durable runtime semantics challenging. Future work may involve preserving scratch directories and re‑attaching mounts.
