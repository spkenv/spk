---
date: 2025-11-29T10:30:00-08:00
repository: spk
git_commit: d2855f085f4ebaa50a2241c013c88c3187cd6076
branch: main
discovery_prompt: "does @.llm/shared/context/2025-11-28-spk-spfs-fuse.md need updated?"
generated_by: "opencode:/discovery"
tags: [context, discovery]
status: complete
last_updated: 2025-11-29
---

# Repo Context Guide: spk (SpFS Runtime + FUSE Integration)

## TL;DR
- SpFS supports two Linux FUSE modes: OverlayFs-with-FUSE (hybrid) and FuseOnly (pure VFS) selected via `runtime::MountBackend` (`crates/spfs/src/runtime/storage.rs`).
- `spfs-enter` orchestrates mount namespaces; when FUSE is enabled it launches the dedicated `spfs-fuse` binary to expose manifests as a live filesystem before overlaying edits (`crates/spfs/src/env.rs`, `status_unix.rs`).
- The `spfs-fuse` daemon (from `crates/spfs-cli/cmd-fuse`) wraps `spfs-vfs` (FUSE filesystem impl) plus `fuser::Session` to serve stack manifests from local/remote repos (`crates/spfs-vfs/src/fuse.rs`).
- Runtime monitor (`spfs-monitor`) heartbeats the FUSE mount so it can self-terminate if clients disappear, avoiding zombie mounts (`crates/spfs/src/monitor.rs`).
- Configuration for worker threads, heartbeats, mount options, secondary repositories lives under `[filesystem]` and `[fuse]` in `spfs` config (`docs/admin/config.md`, `crates/spfs/src/config.rs`).
- Cleanup involves `spfs-enter --exit` unmounting overlay first, then FUSE via `fusermount -u` retries to prevent stale mounts (`crates/spfs/src/env.rs`).

## Quickstart (dev)
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
- **Common commands**:
  - `spfs run <tag> --filesystem-backend overlayfs-with-fuse -- cmd`
  - `spfs shell <tag> --fuse-only`
  - `spfs runtime info <name>` (inspect backend field)
  - `spfs runtime remount <name>` (reinitializes FUSE lower dir)

## How to use (user)
- **Hybrid overlay**: Use overlay for edits but source immutable stack via FUSE to avoid pre-rendering. Example `spfs run plat/base+tools -- filesystem-backend overlayfs-with-fuse -- make`. FUSE exposes stack at runtime-configured `lower_dir`; overlay upper/work capture mutations.
- **Read-only FuseOnly**: `spfs run base --filesystem-backend fuse-only -- cat /spfs/...` yields direct FUSE mount; no overlay upper, so runtime is read-only and mask files not applied.
- **Remote-through reads**: Provide remote hints via config `[filesystem.secondary_repositories]` or CLI `--filesystem-remote origin` so FUSE can fetch objects on demand (`spfs_vfs::Config::remotes`).
- **Heartbeat-sensitive workflows**: If monitor dies (e.g., host reboot), FUSE self-shutdown after heartbeat grace period; re-run `spfs run --rerun <runtime>` to rehydrate environment.

## Repo map
- `crates/spfs/src/runtime/storage.rs`: runtime config/backends + serialization of `MountBackend` and lower/upper dirs.
- `crates/spfs/src/env.rs`: namespace + mount operations; launching/unmounting `spfs-fuse`.
- `crates/spfs/src/status_unix.rs`: high-level init/reinit/durable flows, toggling overlay vs FUSE.
- `crates/spfs-vfs/src/fuse.rs`: actual FUSE filesystem implementing `fuser::Filesystem` over manifests/repositories.
- `crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs`: CLI entrypoint/daemonization, option parsing, heartbeat wiring.
- `crates/spfs/src/config.rs` & `docs/admin/config.md`: user-facing config knobs for `[filesystem]` `[fuse]`.
- `crates/spfs/src/monitor.rs`: heartbeat + PID tracking for FUSE runtimes.
- Tests: `crates/spfs/tests/integration/unprivileged/test_fuse_*`.
- Docs: `docs/spfs/develop/design.md` (process graph) & `docs/spfs/develop/runtime.md` (runtime layering overview).

## Architecture overview
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

## Configuration & environments
- **Config file**: `~/.config/spfs/config.toml` (or `$SPFS_CONFIG`).
  - `[filesystem] backend = "OverlayFsWithFuse" | "FuseOnly"` toggles FUSE usage.
  - `secondary_repositories = [...]` used to seed `spfs-vfs` remote search order.
  - `use_mount_syscalls` irrelevant for FuseOnly but still affects overlay stage when hybrid.
- **FUSE-specific** `[fuse]` keys: `worker_threads`, `max_blocking_threads` (tokio runtime sizing inside `spfs-fuse`); `enable_heartbeat`, `heartbeat_interval_seconds`, `heartbeat_grace_period_seconds` control monitor interplay.
- **Env vars**: `SPFS_FUSE_LOG_FOREGROUND=1` keeps logs in stderr; `SPFS_FILESYSTEM_TMPFS_SIZE` still honored for overlay upper/work even in FUSE hybrid; `SPFS_RUNTIME` identifies active runtime for monitor/resume.
- **Binary resolution**: `env::mount_fuse_onto` uses `which_spfs("fuse")` helper to locate `spfs-fuse` in PATH or installed prefix; ensure packaging installs it.

## Testing & quality
- **Integration scripts** (`crates/spfs/tests/integration/unprivileged`): verify FUSE mount cleanup/resilience (`test_fuse_cleanup.sh`, `test_fuse_du.sh`).
- **Unit coverage**: `spfs-vfs/src/fuse.rs` includes no tests (relies on runtime tests), but CLI and runtime modules have targeted tests. Use `cargo test -p spfs-cli --features fuse-backend` for CLI-level coverage.
- **CI**: Repo cspell + lint workflows but FUSE tests typically gated due to privilege requirements; run manually on Linux hosts.

## Extension points & customization
- **New remotes**: Extend `[filesystem.secondary_repositories]` and ensure relevant `RepositoryHandle`s exist; `spfs-vfs` will iterate in declared order.
- **Additional mount options**: Extend `parse_options_from_args` in `cmd-fuse.rs` to support new `MountOption` or custom key/value pairs.
- **Writable FUSE**: Currently unimplemented (open with `O_WRONLY` rejects). Adding support requires modifications in `Filesystem::open` and handle types to stage writes, plus overlay coordination.
- **Heartbeat hooks**: `spfs-monitor` `enable_heartbeat` gating; can integrate alternative liveness signals by extending `monitor.rs` tick logic if more domains need to notify `spfs-fuse`.

## Operational notes
- **Deployment**: `spfs-fuse` must run privileged (root or allow-other) to permit `allow_other` mounts; packaging sets capabilities via RPM spec `%caps(cap_sys_admin+ep)` on `/usr/local/bin/spfs-fuse` (`spfs.spec`).
- **Observability**: `spfs-fuse` logs to `/tmp/spfs-runtime/fuse.log` (default) + syslog; adjust via CLI `--log-file`. Heartbeat failures logged at WARN + Sentry event via `warn_and_sentry_event!`.
- **Performance**: `spfs-vfs` caches entire manifest tree in memory (inode map). For huge platforms, expect larger RSS; consider flattening/limiting stack. Worker threads default `min(num_cpus, 8)`; increase to improve concurrency at cost of CPU.
- **Cleanup pitfalls**: Lazy unmount used when remounting overlay because FUSE may still be busy. Monitor ensures `fusermount` gets multiple attempts; if `spfs-fuse` hung (monitor crash) there is fallback to heartbeat-induced shutdown.

## LLM working set
1. `crates/spfs/src/runtime/storage.rs` – defines `MountBackend`, runtime config, detection helpers (`is_backend_fuse`, `rotate_lower_dir`).
2. `crates/spfs/src/env.rs` – `mount_fuse_*`, `unmount_env_*`, namespace guards.
3. `crates/spfs/src/status_unix.rs` – initialization/remount flows with FUSE branches.
4. `crates/spfs-vfs/src/fuse.rs` – FUSE filesystem logic (lookup/read/open semantics, repo access).
5. `crates/spfs-cli/cmd-fuse/src/cmd_fuse.rs` – daemon lifecycle, CLI options, heartbeat integration.
6. `crates/spfs/src/config.rs` & `docs/admin/config.md` – user-tunable settings.
7. `crates/spfs/src/monitor.rs` – heartbeat + PID tracking interplay.
8. `docs/spfs/develop/design.md` & `runtime.md` – conceptual overview of process graph & runtime layering.
9. `crates/spfs/tests/integration/unprivileged/test_fuse_cleanup.sh` – expected cleanup behavior.
10. `spfs.spec` & `Makefile.linux` – packaging/capability setup for FUSE binaries.

## Open questions
- How does `spfs-fuse` authenticate/authorize access to secondary remotes? (Inspect repository credential handling in `spfs::storage`.)
- What retry/backoff exists when repo reads fail mid-request? (Search `Handle::read`/`read_dir` in `spfs-vfs/src/fuse.rs`.)
- Are there plans for write support via FUSE (overlayless edits)? (Track TODO in `cmd-fuse.rs` around `rw` option.)
- How does Windows WinFSP backend differ in capabilities vs FuseOnly? (Review `crates/spfs-cli/cmd-winfsp` + `runtime/winfsp.rs`.)
- What telemetry is emitted when heartbeat shutdown occurs? (Follow `warn_and_sentry_event!` definitions in `spfs_cli_common`.)
