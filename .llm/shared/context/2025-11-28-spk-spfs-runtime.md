---
date: 2025-11-29T00:00:00-08:00
repository: spk
git_commit: d2855f085f4ebaa50a2241c013c88c3187cd6076
branch: main
discovery_prompt: "@.llm/shared/context/2025-11-28-spk-spfs-runtime.md update"
generated_by: "opencode:/discovery"
tags: [context, discovery]
status: complete
last_updated: 2025-11-29
---

# Repo Context Guide: spk (SpFS Filesystem Initialization)

## TL;DR
- `spfs run` / `spfs shell` resolve `EnvSpec` stacks, sync repos, persist runtime metadata, then exec the privileged `spfs-enter` helper (`crates/spfs-cli/main/src/cmd_run.rs`, `crates/spfs/src/bootstrap.rs`).
- Runtime state (layers, editability, durability, annotations, live layers) is serialized into the runtime repository so monitors and future sessions can recover it (`crates/spfs/src/runtime/storage.rs`).
- Overlay-based backends flatten stacks as needed, render manifests to real directories, and select kernel options (e.g., `lowerdir+`) based on `/sbin/modinfo overlay` output (`crates/spfs/src/resolve.rs`, `crates/spfs/src/runtime/overlayfs.rs`).
- Namespace + mount orchestration happens via `env.rs` guards plus `spfs-enter`; threads must stay put because mount namespaces are per-thread (`crates/spfs/src/env.rs`, `crates/spfs/src/status_unix.rs`).
- Startup scripts rewrite shell env vars that get lost during privilege changes, and source `/spfs/etc/spfs/startup.d` before handing control to the user (`runtime/startup_sh.rs`, `runtime/startup_csh.rs`).
- Live layers bind host paths into `/spfs` for per-run customization; durable runtimes relocate upper/work dirs into repo-owned storage and can be rerun safely (`runtime/live_layer.rs`, `runtime/storage.rs`).

## Quickstart (dev)
- **Prereqs**
  - Linux with overlayfs + user namespaces (or Windows with WinFsp build).
  - Rust toolchain from `rust-toolchain.toml`, `cargo` + `rustfmt`, `clippy`.
  - `rsync`, `/sbin/modinfo overlay`, and `setcap`/`sudo` to grant `spfs-enter` capabilities; optional WinFsp for Windows.
- **Setup**
  - `cargo build -p spfs-cli -p spfs-enter -p spfs-render -p spfs-monitor`.
  - `make setcap bindir=target/debug` (or release) so `spfs-enter` can `unshare`/`mount`.
  - Initialize storage repo: `spfs repo init` or manually create `{objects,payloads,tags}` tree; configure remotes in `~/.config/spfs/config.toml` or via CLI.
- **Run**
  - `target/debug/spfs run base+tools -- bash` -> resolves refs, renders, enters runtime.
  - `SPFS_FILESYSTEM_TMPFS_SIZE=20G spfs run sdk -- shell` to grow tmpfs.
  - `spfs shell -` for an empty editable runtime.
  - `spfs run --keep-runtime --runtime-name dev image -- zsh` to keep/durable.
- **Test**
  - `cargo test -p spfs -- runtime::` (needs Linux + privileges; ignored tests for overlay operations).
  - `cargo test -p spfs-cli cmd_run::` to vet CLI orchestration.
  - `make test CRATES=spfs` or `cargo nextest run -p spfs` for faster loops.
- **Common commands**
  - `spfs run <refs> -- <cmd>` / `spfs shell <refs>` / `spfs edit`.
  - `spfs runtime list|info|remove|prune` for runtime storage management.
  - `spfs render <refs>` to pre-render layers.
  - `spfs clean --remove-durable <name>` to delete durable upper dirs.

## How to use (user)
- **Launch builds**: `spfs run platform+patch -- my-build.sh` resolves stack, ensures layers locally (or via proxy when using FUSE), mounts `/spfs`, runs script.
- **Interactive shells**: `spfs shell workspace/tag --edit -- zsh` for editable session; startup scripts propagate env overrides and source `/spfs/etc/spfs/startup.d/*.sh`.
- **Durable runtimes**: `spfs run --keep-runtime --runtime-name dev tools -- bash`; re-enter with `spfs run --rerun dev -- bash` (optionally `--force` to clean stale owner/monitor state).
- **Live layers**: include YAML spec path inside `EnvSpec` (e.g., `project/live.spfs.yaml`); CLI loads, validates, and `env.rs` bind-mounts host content into `/spfs`.
- **Monitoring/cleanup**: `spfs monitor` watches owner PID; `spfs-enter --exit` tears down overlay + tmpfs; durability keeps upper/work dir until `spfs runtime remove` or `spfs-clean --remove-durable` runs.

## Repo map
- `crates/spfs/src/runtime/`: runtime metadata, live layers, overlay runtime logic, startup scripts.
- `crates/spfs/src/status_{unix,win}.rs`: platform-specific runtime lifecycle entrypoints.
- `crates/spfs/src/env.rs`: namespace, privilege, mount orchestration.
- `crates/spfs/src/resolve.rs`: stack resolution, flattening, rendering, `which_spfs` helpers.
- `crates/spfs-cli/main/src/cmd_run.rs`: CLI entrypoint for `spfs run`/`shell` (plus fixtures/tests).
- `docs/spfs/develop/{runtime.md,design.md}`: narrative design, process diagrams.
- `crates/spfs/src/runtime/live_layer.rs`: schema + validation for live layer bind mounts.
- **Read first**: `runtime/storage.rs`, `status_unix.rs`, `env.rs`, `resolve.rs`, `docs/spfs/develop/runtime.md`.

## Architecture overview
- **CLI orchestration**: `CmdRun::run` acquires repository handles, loads/creates runtime, syncs refs (via `EnvSpec` + `Sync` helper), applies annotations, decides backend, persists runtime, and builds final `Command` via `spfs::build_command_for_runtime`.
- **Runtime metadata layer**: `runtime::Storage` persists `Data { Status, Config }` objects inside runtime repo as blobs referenced by special tags, enabling multi-process coordination and `spfs runtime list/info` to introspect state.
- **Resolution/render pipeline**: `resolve.rs` converts stacks into `graph::Layer`s, flattening overly large sets (group size 7) until overlay arg length fits kernel limits; renders via `spfs-render` (copy strategy for durable runtimes) and records `RenderResult` paths.
- **Namespace + mount control**: `env::RuntimeConfigurator` guards transitions into new mount namespaces, toggles root privileges, mounts tmpfs runtime roots, overlayfs (syscalls vs CLI path), optional FUSE backends, and masks deletions using overlay whiteouts.
- **Durability & live layers**: `runtime::Runtime::setup_durable_upper_dir` moves upper/work dirs into repo-local `DURABLE_EDITS_DIR`, uses `rsync` to copy contents, and toggles overlay remount; live layers ensure mountpoints exist by creating temp manifests inserted at bottom of stack.
- **Startup & child exec**: Startup scripts rewrite env vars lost when `spfs-enter` `exec`s privileged helpers, then run user shell/command; `SPFS_METRICS_SYNC_TIME_SECS` surfaces sync latency.

## Key components (deep links)
### Runtime storage (`crates/spfs/src/runtime/storage.rs`)
- **Purpose**: Define runtime schema (`Data`, `Status`, `Config`), persist via repo tags, handle annotations, live layers, durable configs, startup scripts, env overrides.
- **Entry points**: `Storage::create_runtime`, `Runtime::save_state_to_storage`, `OwnedRuntime::upgrade_as_owner/monitor`, `Runtime::prepare_live_layers`, `Runtime::ensure_startup_scripts`.
- **Invariants**: `Status.stack` ordered bottom→top; `flattened_layers` keeps GC roots; durable runtimes require local FS upper/work; `live_layers` dest directories must exist before mounting.

### Namespace + mount control (`crates/spfs/src/env.rs`, `crates/spfs/src/status_unix.rs`)
- **Purpose**: Manage `unshare`, `setns`, `setuid/euid`, overlay/fuse mounting, mask deletions, change-to-durable operations.
- **Key abstractions**: `RuntimeConfigurator<UserState, NamespaceState>` ensures compile-time enforcement of root/mount-state, `ThreadIsInMountNamespace` vs `ProcessIsInMountNamespace` for thread safety.
- **Flow**: `initialize_runtime` -> `Runtime::prepare_live_layers` -> enter namespace -> mount tmpfs runtime dir -> render overlay -> mask deletes -> run startup; `change_to_durable_runtime` reuses same pipeline but only remounts overlay portion.

### Resolution/render pipeline (`crates/spfs/src/resolve.rs`)
- **Purpose**: Evaluate `EnvSpec`, resolve tags/digests to layers, flatten stacks, call `spfs-render`, compute manifests for masking.
- **Notables**: Flatten group size 7 to amortize work; durable runtimes force copy renders; fallback proxy repo merges local + remote handles without syncing tag streams (prevents `spfs clean` retention issues).
- **Sys integration**: `overlayfs_available_options` reads `/sbin/modinfo overlay`, enabling new kernel flags like `lowerdir+` when available.

### CLI + bootstrap (`crates/spfs-cli/main/src/cmd_run.rs`, `crates/spfs/src/bootstrap.rs`)
- **Purpose**: Parse flags, gather annotations (supports YAML/JSON files), create/reuse runtimes, set editability, sync refs (with digest-only sync to avoid tag bloat), build `spfs-enter` commands for final exec/monitor.
- **Hooks**: `--keep-runtime` toggles durability; `--rerun` paths update runtime state, `--force` resets owner/monitor fields; env var `SPFS_KEEP_RUNTIME` is honored.

### Live layers & startup scripts (`crates/spfs/src/runtime/live_layer.rs`, `runtime/startup_sh.rs`)
- **Purpose**: Bind host directories/files into `/spfs` for rapid iteration; ensure mountpoint existence and validation.
- **Startup**: Shell templates re-export env overrides, source `/spfs/etc/spfs/startup.d/*.sh`, optionally print `SPFS_SHELL_MESSAGE`, and exec user command.

## Configuration & environments
- **Config sources**: `spfs::Config::current()` loads workspace/user config (default `~/.config/spfs/config.toml`); repo-level `workspace.spk.{yaml,yml}` describes packages/layers.
- **Filesystem backends**: `MountBackend` enum selects overlay renders vs overlay+fuse vs fuse-only vs WinFsp; `requires_localization()` controls when remote refs must be synced locally.
- **Env vars**: `SPFS_RUNTIME` (active runtime name), `SPFS_FILESYSTEM_TMPFS_SIZE`, `SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING`, `SPFS_SHELL_MESSAGE`, `SPFS_DEBUG`, `SPFS_DIR_PREFIX`, `SPFS_METRICS_SYNC_TIME_SECS` (exported by CLI), `SPFS_KEEP_RUNTIME`.
- **Secrets**: Remotes may include credentials; runtime metadata stored in repo tags (ensure secure repo perms). Durable upper dirs live under local repo `DURABLE_EDITS_DIR` (cannot be on NFS due to overlayfs requirements).

## Testing & quality
- **Unit tests**: `runtime/*_test.rs` (storage, overlayfs, live layers), `resolve_test.rs`, CLI fixtures under `crates/spfs-cli/main/src/cmd_run_test.rs`.
- **Integration**: Overlay + namespace tests often `#[ignore]` due to needing privileges—run manually on Linux host with `sudo make test`.
- **CI**: GitHub workflows run lint/test/cspell; ensure `cargo fmt`, `cargo clippy --workspace`, and `make lint` pass before PR.
- **Fixtures**: `crates/spfs/fixtures.rs`, CLI fixtures for repo scaffolding.

## Extension points & customization
- **Storage**: Add new repository backends via `crates/spfs/src/storage/*`, implement `Repository` traits, then wire into config/resolution.
- **Mount backends**: Extend `MountBackend` + `env.rs` mount paths for new virtualization layers; update CLI flag validation.
- **Live layers**: Extend `LiveLayerContents` (currently only bind mounts) for new mount types if desired.
- **Startup hooks**: Drop scripts into rendered layers under `/spfs/etc/spfs/startup.d`; update `startup_{sh,csh,ps}.rs` to support additional shells.
- **CLI commands**: Mirror `cmd_run.rs` structure when adding new runtime operations; reuse `spfs_cli_common::Sync` for repo interactions.

## Operational notes
- **Deployment**: `spfs-enter` binary must retain `CAP_SYS_ADMIN` (Linux) or equivalent; install via `make install` + `make setcap`. On Windows, ship WinFsp driver + `runtime/winfsp.rs` backend.
- **Durable cleanup**: `spfs runtime remove <name>` invokes `spfs-clean --remove-durable` to delete repo-backed upper dirs; if manual, run `spfs-clean --remove-durable <name> --runtime-storage <url>`.
- **Observability**: CLI records sync duration in `SPFS_METRICS_SYNC_TIME_SECS`; Sentry integration configured via global config (see `spfs::Config::sentry`).
- **Performance**: Overlay arg length is kernel-limited; flattening merges layers and writes manifests to repo (kept alive via `flattened_layers` set). Durable runtimes disable overlay `index=on` and enforce copy renders to avoid hard-link semantics issues.
- **Cross-platform**: `status_win.rs` + `runtime/winfsp.rs` handle WinFsp specifics; ensure parity with Linux flows (masking handled differently because Win overlay semantics differ).

## LLM working set
1. `crates/spfs/src/runtime/storage.rs` – runtime schema, persistence, durability, live layers.
2. `crates/spfs/src/status_unix.rs` & `status.rs` – lifecycle entrypoints (init, remount, durable, exit).
3. `crates/spfs/src/env.rs` – namespace, privilege, mount operations, mask handling.
4. `crates/spfs/src/resolve.rs` – stack resolution, flattening, rendering helper.
5. `crates/spfs-cli/main/src/cmd_run.rs` – CLI orchestration, annotation parsing, sync.
6. `crates/spfs/src/runtime/live_layer.rs` – bind mount schema/validation.
7. `crates/spfs/src/runtime/startup_sh.rs` (+ `startup_csh.rs`, `startup_ps.rs`) – shell bootstrap logic.
8. `docs/spfs/develop/runtime.md` – runtime semantics narrative.
9. `docs/spfs/develop/design.md` – system architecture & process diagram.
10. `crates/spfs/src/runtime/overlayfs.rs` – overlay option detection, kernel capability handling.

## Open questions
- `spfs-enter` internals live outside this crate; inspect its source (likely `crates/spfs-enter`) to document exact namespace + process orchestration sequence.
- Windows backend parity: confirm `runtime/winfsp.rs` provides the same live-layer + durability guarantees; document deviations.
- Fuse backend (`MountBackend::OverlayFsWithFuse` / `FuseOnly`) is behind a crate feature; enumerate how to enable/build/test it.
- Runtime monitoring (`spfs monitor`) behavior is implied but not covered here—trace `crates/spfs-cli/main/src/cmd_runtime*.rs` for lifecycle automation.
- Telemetry hooks: Sentry/metrics integration referenced in config but detailed flows aren’t documented; audit `crates/spfs/src/config.rs` + sentry crates.
