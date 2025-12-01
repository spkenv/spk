---
date: 2025-11-28T10:45:00-08:00
repository: spk
git_commit: 5c32e2093677ef44b7fc8b227ae20ccec29a1069
branch: main
discovery_prompt: "How does the FUSE layer work with Windows which does not have the namespace mounts and capabilities system?"
generated_by: "opencode:/discovery"
tags: [context, discovery]
status: complete
last_updated: 2025-11-28
---

# Repo Context Guide: spk (Windows WinFSP backend for SpFS)

## TL;DR
- Windows hosts do **not** run Linux FUSE + namespaces; SpFS swaps to the `WinFsp` mount backend defined in `MountBackend::WinFsp`.
- `spfs-run` still builds runtime metadata, but `RuntimeConfigurator::mount_env_winfsp` launches the `spfs-winfsp` binary instead of `spfs-fuse`/overlayfs.
- The `spfs-winfsp service` process mounts a single WinFSP filesystem at `C:\spfs`; a gRPC control plane routes per-process views.
- `spfs-winfsp mount` (invoked per runtime) tells the service which EnvSpec to expose to the runtime owner PID tree; routing is done entirely in user space by checking Windows process parentage.
- Because Windows lacks overlayfs, masks, and tmpfs workdirs, the WinFSP backend is currently **read-only** and several lifecycle commands are unimplemented/TODO.
- Config knobs live under `[filesystem] backend = "WinFsp"` plus `SPFS_WINFSP_LISTEN_ADDRESS`; remotes are still honored for read-through fetches.

## Quickstart (dev)
- **Prereqs**: Windows 10/11, [WinFSP](https://winfsp.dev) driver, Rust toolchain via `rustup`, Chocolatey packages `make`, `protoc`, `llvm` (see `README.md`), FlatBuffers `flatc`, and Git Bash or PowerShell.
- **Setup**:
  1. `rustup default stable-x86_64-pc-windows-msvc`.
  2. `cargo build -p spfs -p spfs-cli` with `--features winfsp-backend` (workspace `Cargo.toml` wires feature propagation).
  3. Ensure `spfs-winfsp.exe` is in `PATH` (built under `target\debug`).
- **Run**:
  - Start the WinFSP service (one per host): `target\debug\spfs-winfsp service --mountpoint C:\spfs`. This mounts an always-on filesystem and opens gRPC on `127.0.0.1:37737` by default.
  - Launch a runtime: `spfs run my/tag --filesystem-backend winfsp -- powershell`. `spfs-enter` will call `spfs-winfsp mount --root-process <PID> my/tag` under the hood.
- **Test**:
  - Limited automated coverage exists; run `cargo test -p spfs --features winfsp-backend` for compile-time regressions (most runtime tests are Linux-only).
  - Manual smoke: `spfs ls my/tag` then `spfs run my/tag -- cmd /c dir C:\spfs` to confirm routing.
- **Common commands**:
  - `spfs-winfsp service [--stop]`
  - `spfs-winfsp mount --root-process <pid> <ref>` (normally auto-invoked)
  - `spfs run <refs> --filesystem-backend winfsp -- <cmd>`
  - `spfs config show filesystem.backend`

## How to use (user)
- **Launching environments**: `spfs run tools/base --filesystem-backend winfsp -- powershell` starts a shell whose `/spfs` view is served by WinFSP; files live under `C:\spfs` for Windows paths.
- **Inspecting references**: `spfs ls <ref> path\in\runtime` works identically because repository logic is platform-agnostic.
- **Managing the service**: If `spfs run` fails with connection refused, start the service manually: `spfs-winfsp service --listen 127.0.0.1:37737 --mountpoint C:\spfs`.
- **Multiple runtimes**: WinFSP router lets several runtimes share the single mount. Each runtime’s owning process ID is registered so that file operations from that PID tree see the correct manifest stack.
- **Cleanup**: Because durable runtime teardown is unimplemented on Windows, exit shells normally and manually stop the service if necessary (`spfs-winfsp service --stop`).

## Repo map
- `crates/spfs/src/runtime/storage.rs` – defines `MountBackend::WinFsp` and runtime config flags.
- `crates/spfs/src/env_win.rs` – Windows-only `RuntimeConfigurator` with `mount_env_winfsp` logic.
- `crates/spfs/src/status_win.rs` – lifecycle hooks for initialize/remount/exit (mostly TODO except initialize).
- `crates/spfs/src/monitor_win.rs` – placeholder for monitor process support.
- `crates/spfs-cli/cmd-winfsp/` – CLI entrypoint providing `service` and `mount` subcommands.
- `crates/spfs-vfs/src/winfsp/` – WinFSP filesystem implementation (Service, Router, Mount, Handle modules).
- `README.md` & `docs/admin/install.md` – note WinFSP dependency and experimental support.

## Architecture overview
1. **Runtime creation (unchanged)**: `spfs run` builds runtime metadata, resolves stack, and persists `runtime::Data` exactly like Linux.
2. **Mount backend selection**: On Windows, `MountBackend::WinFsp` is the default. `environment::RuntimeConfigurator::mount_env_winfsp` is invoked instead of overlay/fuse mounting.
3. **Service orchestration**:
   - `spfs-winfsp service` initializes `winfsp::host::FileSystemHost`, mounts `C:\spfs`, spins a gRPC server (tonic) on `127.0.0.1:37737`. It opens a `ProxyRepository` that wraps the configured local repo plus remotes for read-through fetches.
   - Host thread is non-Send; it runs in a dedicated thread and can be shutdown via async channel.
4. **Per-runtime binding**:
   - `spfs-winfsp mount` gRPC request carries the runtime owner PID (`root_pid`) and EnvSpec string.
   - `Service::router` builds a new `Mount` per PID: loads manifest via repo stack, pre-allocates inodes, caches entries.
   - Router intercepts every filesystem operation, determines caller PID via `winfsp_sys::FspFileSystemOperationProcessIdF`, walks parent stack (`get_parent_pids`) to match registered root PID, and dispatches to the correct `Mount` instance. This emulates Linux mount namespaces purely in user space.
5. **File access**:
   - `Mount` resolves files either by opening payloads from local FS repo or streaming from secondary repositories via `RepositoryHandle::open_payload` (async tasks on tokio runtime; results bridged back to WinFSP).
   - All mounts are read-only (`FILE_ATTRIBUTE_READONLY`), as Windows backend lacks overlay upper/work directories.
6. **Teardown**:
   - Currently only service shutdown (`spfs-winfsp service --stop`) is implemented. `status_win.rs` contains TODOs for remount, exit, durable flows, so cleanup is manual.

## Key components (deep links)
### `MountBackend::WinFsp` (`crates/spfs/src/runtime/storage.rs`)
- **Purpose**: Flag runtimes to use WinFSP backend; indicates localization is unnecessary.
- **Behavior**: `requires_localization()` returns false, so stacks can stream from remotes.
- **Impact**: CLI must ensure WinFSP binaries exist; `spfs run` automatically chooses this backend on Windows builds.

### `RuntimeConfigurator::mount_env_winfsp` (`crates/spfs/src/env_win.rs`)
- **Purpose**: Replace overlay namespaces with WinFSP service calls.
- **Flow**: Retrieves runtime owner PID, builds EnvSpec string from stack, locates `spfs-winfsp.exe`, and runs `spfs-winfsp mount --root-process <pid> <spec>`.
- **Error handling**: Surfaces missing owner, missing binary, or non-zero exit status as runtime errors.

### `spfs-winfsp` CLI (`crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs`)
- **`service` subcommand**: Initializes WinFSP, opens repository stack, spawns gRPC + host threads, handles Ctrl-C, and supports `--stop` by calling `proto::VfsService::shutdown`.
- **`mount` subcommand**: Ensures service is running (auto-spawns if connection refused), determines parent PID (or provided `--root-process`), and sends `MountRequest` over gRPC. Uses `DETACHED_PROCESS` flag to background the service when autospawned.

### WinFSP filesystem core (`crates/spfs-vfs/src/winfsp`) 
- **`Service`**: Wraps repository stack, host controller, and router; exposes gRPC methods `mount` and `shutdown`.
- **`Router`**: Maintains map of root PID ➜ `Mount`. Routes WinFSP callbacks by inspecting the calling process stack, effectively simulating namespaces.
- **`Mount`**: Implements `winfsp::filesystem::FileSystemContext` with pre-allocated inodes, attribute mapping, and read-only file handles (either `BlobFile` or streaming handles). Uses async tasks to fetch payloads, bridging to WinFSP via channels.

### Windows lifecycle stubs (`crates/spfs/src/status_win.rs`, `monitor_win.rs`)
- `initialize_runtime` calls `mount_env_winfsp`, but `remount_runtime`, `exit_runtime`, `make_runtime_durable`, and monitoring helpers are `todo!()`. This documents current limitations: durable runtimes, cleanup, and monitors are not yet implemented on Windows.

## Configuration & environments
- Config file: `%AppData%\spfs\spfs.toml` (or `%ProgramData%\spfs\spfs.toml`). Relevant keys:
  - `[filesystem] backend = "WinFsp"` (default on Windows builds).
  - `secondary_repositories = ["origin", ...]` – forwarded to WinFSP repo stack for read-through streaming.
  - `[fuse]` block is ignored; `[winfsp]` block does **not** exist yet (use env vars below).
- Environment variables:
  - `SPFS_WINFSP_LISTEN_ADDRESS` overrides `127.0.0.1:37737` for both service and mount commands.
  - `SPFS_MONITOR_FOREGROUND_LOGGING` is referenced but monitor implementation is TODO.
  - Standard `SPFS_*` vars (`SPFS_RUNTIME`, etc.) still apply.
- Service mountpoint defaults to `C:\spfs`, configurable via CLI `--mountpoint`.

## Testing & quality
- Automated tests mainly target Unix; Windows modules lack dedicated unit tests.
- Many Windows-specific files contain `todo!()` (status, monitor, renderer), signaling incomplete functionality.
- CI workflows (`.github/workflows/*.yml`) run on Linux; no Windows CI stage currently validates WinFSP backend.
- Manual QA recommendation: run `spfs-winfsp service` + `spfs run` smoke tests on developer Windows hosts before relying on production flows.

## Extension points & customization
- **Service routing**: `Router` currently locks routes per root PID; to support user sessions or multi-tenant policies, extend router to include security descriptors or session tokens.
- **Writable support**: Would require implementing overlay-like semantics (e.g., per-runtime scratch location) inside `Mount` and handling `create/write`. Currently all methods return read-only attributes.
- **Monitor hooks**: `monitor_win.rs` is empty; adding watchdog functionality will allow automatic cleanup akin to Linux monitor.
- **Config surface**: No `[winfsp]` config table yet; add one in `spfs::config` to expose listen address, mountpoint, or routing modes.

## Operational notes
- **Deployment model**: Single WinFSP driver mount per host. Service must run under an account with rights to mount the driver. Since there are no namespaces, all processes can technically access `C:\spfs`; isolation relies on router logic rejecting unknown PID stacks.
- **Observability**: `spfs-winfsp` logs via `tracing` (defaults to syslog / stderr). No Windows Event Log integration yet.
- **Performance**: All manifest entries are pre-loaded into memory per mount; large stacks increase RAM usage. Streaming payloads uses async tasks plus oneshot channels; heavy remote traffic may block operations if remotes are slow.
- **Limitations**: No edit mode, durable runtimes, or cleanup automation. Deletions/masks rely on manifest data but there is no overlay mask directory, so masked entries simply return `ENOENT` via router.

## LLM working set
1. `crates/spfs/src/runtime/storage.rs` – `MountBackend` definitions and runtime config.
2. `crates/spfs/src/env_win.rs` – Windows configurator and WinFSP mount call.
3. `crates/spfs/src/status_win.rs` – Windows lifecycle (TODO markers + init).
4. `crates/spfs-cli/cmd-winfsp/src/cmd_winfsp.rs` – CLI orchestration of service/mount commands.
5. `crates/spfs-vfs/src/winfsp/mod.rs` – Service, Config, HostController wiring.
6. `crates/spfs-vfs/src/winfsp/router.rs` – Process routing logic simulating namespaces.
7. `crates/spfs-vfs/src/winfsp/mount.rs` & `handle.rs` – File operations, attribute mapping, streaming.
8. `docs/admin/install.md` & `README.md` (Windows sections) – user-facing requirements and limitations.
9. `Cargo.toml` (workspace + `features.winfsp-backend`) – build graph for enabling backend.
10. `TODO.md` & Windows monitor/renderer stubs – highlight unimplemented functionality.

## Open questions
- How will durable runtimes and editable overlays be supported on Windows? (`status_win.rs` and `renderer_win.rs` are TODOs.)
- What replaces `spfs-monitor` on Windows once `monitor_win.rs` is implemented? Need spec for detecting orphaned PID trees without namespaces.
- Can router-based isolation be bypassed (e.g., other processes reading `C:\spfs`)? Audit required for security guarantees.
- Is there a plan to expose WinFSP-specific config (listen addr, mountpoint) in `spfs.toml` rather than CLI/env only?
- Automated testing/CI for WinFSP backend is missing; identify strategy (GitHub Actions Windows runners?).
