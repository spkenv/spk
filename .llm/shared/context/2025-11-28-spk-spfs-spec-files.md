---
date: 2025-11-28T00:00:00-08:00
repository: spk
git_commit: 263ab61c28122e596335bb841f892097798e9289
branch: feature/macos-fuse-auto-start
discovery_prompt: "spfs has some special configuration files that can be used to specify layers/tags to load?"
generated_by: "opencode:/discovery"
tags: [context, discovery]
status: complete
last_updated: 2025-12-07
---

# Repo Context Guide: spk (SpFS run/layer spec files)

## TL;DR
- SpFS accepts **EnvSpec strings** that can include human tags, digests, and *special YAML files* (`*.spfs.yaml`) to describe runtime stacks.
- `runspec.spfs.yaml` (aka *run spec*) enumerates an ordered list of refs (`api: spfs/v0/runspec`) so long command lines become reusable files.
- `layer.spfs.yaml` (aka *live layer spec*, `api: spfs/v0/livelayer`) binds host paths into `/spfs`, enabling live-edit overlays without capturing new layers.
- Parsing lives in `crates/spfs/src/tracking/env.rs`; it flattens nested run specs, validates live layers, and guards against recursive includes.
- `spfs run` (CLI) wires these specs into runtime creation, syncing tags/digests, rendering overlays, and mounting live layers via `runtime::LiveLayer`.
- Docs under `docs/spfs/usage.md` explain user workflows; internal tests (`env_test.rs`, `live_layer_test.rs`) lock formats.

## Quickstart (dev)
- **Prereqs**: Rust toolchain, `cargo build -p spfs-cli`, local spfs repo initialized, Linux permissions (`make setcap bindir=target/debug`), WinFsp on Windows, or FUSE on macOS.
- **Setup**:
  1. Create `$PWD/examples/layers/runspec.spfs.yaml` with `api: spfs/v0/runspec` and `layers: [tag-or-digest, ...]`.
  2. Create `$PWD/examples/live/layer.spfs.yaml` with `api: spfs/v0/livelayer` and `contents` bind mounts that exist under the same directory.
- **Run**:
  - `spfs run /abs/path/to/runspec.spfs.yaml -- bash` loads that stack.
  - `spfs run tag+/abs/path/to/project/layer.spfs.yaml -- ls /spfs/project` overlays host directories.
- **Test**:
  - `cargo test -p spfs tracking::env` verifies run-spec parsing.
  - `cargo test -p spfs runtime::live_layer` validates bind-mount rules.
- **Common commands**:
  - `spfs run REF[+REF...] -- <cmd>`
  - `spfs shell REF --edit`
  - `spfs log <tag>` to inspect history for run-spec entries.

## How to use (user)
- **Reusable layer stacks**:
  ```yaml
  # /projects/show/runspec.spfs.yaml
  api: spfs/v0/runspec
  layers:
    - shows/base-platform
    - shows/base-platform~1   # any EnvSpec string works
    - 6PJDUUENJYFFDKZWRKHDUXK4FGGZ7FHDYMZ7P4CXORUG6TUMDJ7A====
  ```
  Run with `spfs run /projects/show/runspec.spfs.yaml -- my-build.sh`.
- **Mixing refs**: `spfs run toolchain+/projects/show/runspec.spfs.yaml+feature-tag -- bash` merges inline refs plus file-based ones in order.
- **Live layers for host checkouts**:
  ```yaml
  # ~/src/game/layer.spfs.yaml
  api: spfs/v0/livelayer
  contents:
    - bind: repo          # relative to the YAML file
      dest: /spfs/game
    - bind: data/assets
      dest: /spfs/game/assets
  ```
  Launch: `spfs run base+/Users/me/src/game/layer.spfs.yaml -- shell --edit`.
- **Multiple files**: `spfs run run1.spfs.yaml+run2.spfs.yaml+~/src/game` (directories auto-resolve to `layer.spfs.yaml`).

## Repo map
- `docs/spfs/usage.md` – authoritative user docs for run specs & live layers.
- `docs/spfs/develop/design.md` – process overview showing where run specs fit in.
- `crates/spfs/src/tracking/env.rs` – EnvSpec parser, spec-file detection, recursion guard.
- `crates/spfs/src/runtime/live_layer.rs` – schema + validation for live layer YAML.
- `crates/spfs-cli/main/src/cmd_run.rs` – CLI glue that loads EnvSpec, syncs refs, attaches live layers.
- Tests: `crates/spfs/src/tracking/env_test.rs`, `crates/spfs/src/runtime/live_layer_test.rs`.
- “Read this first”: `docs/spfs/usage.md` and `crates/spfs/src/tracking/env.rs`.

## Architecture overview
1. **CLI input**: `spfs run`/`shell` accept EnvSpec strings (tags/digests separated by `+`). Absolute paths ending in `.spfs.yaml` or directories containing `layer.spfs.yaml` are treated specially.
2. **EnvSpec parsing** (`tracking::env`): breaks the string, tries digest → partial digest → spec file → tag. When encountering run-spec files it loads, recurses, and flattens referenced layers; for live layers it keeps structured metadata.
3. **Sync/resolve** (`cmd_run.rs`): resolves digests via repos, syncing remotes if overlay backend needs local copies. Tags converted to digests before sync to avoid polluting local tag streams.
4. **Runtime config** (`runtime::Runtime`): stores ordered digest stack plus `LiveLayer` entries, editability, annotations, backend choice.
5. **Mounting** (`runtime/live_layer.rs`, `env.rs`): during runtime init, live layers are validated and bind-mounted over `/spfs` after overlay stack is mounted (platform-specific: Linux uses mount syscalls, Windows uses WinFsp, macOS uses FUSE).

## Key components (deep links)
### EnvSpec parser (`crates/spfs/src/tracking/env.rs`)
- **Purpose**: Convert CLI spec strings into typed `EnvSpecItem`s (tag, digest, runspec file, live layer file).
- **Entry points**: `EnvSpec::parse`, `SpecFile::parse`, `EnvLayersFile::flatten`.
- **Invariants**: `.spfs.yaml` paths must be absolute; directories expand to `layer.spfs.yaml`; recursion is prevented via `SEEN_SPEC_FILES` mutex.
- **Notable logic**: `SpecApiVersionMapping` peeks at `api` field to distinguish run spec vs live layer; flattening ensures nested run specs become simple stacks.

### Run-spec files (`EnvLayersFile`)
- **Purpose**: Represent ordered layer/tag lists from YAML (supports mixing digests, tags, other spec files).
- **Usage**: CLI accepts `/abs/path/runspec.spfs.yaml`; `layers` entries parsed as EnvSpecItem so they can nest.
- **Reference**: `docs/spfs/usage.md` (Run spec section); tests in `tracking/env_test.rs` ensure files load.

### Live layer specs (`runtime/live_layer.rs`)
- **Purpose**: Bind host directories/files into `/spfs` at runtime without capturing them.
- **API**: `api: spfs/v0/livelayer`, `contents` array of `{bind|src, dest}` entries.
- **Validation**: `LiveLayer::set_parent_and_validate` canonicalizes paths, ensures sources exist under the spec directory, and restricts to relative binds for security.

### CLI runtime command builder (`crates/spfs-cli/main/src/cmd_run.rs`)
- **Purpose**: Materialize EnvSpec into runtime state, manage remote sync, annotate runtimes, and hand off to `spfs-enter`.
- **Highlights**: `reference.load_live_layers()` filters live layers; `runtime.push_digest` collects resolved digests; `build_command_for_runtime` injects metrics env vars.

## Configuration & environments
- **File discovery**: SpFS config read from `/etc/spfs.toml` then `~/.config/spfs/spfs.toml` (`docs/admin/config.md`). Governs storage roots, remotes, filesystem backend.
- **Env vars**: `SPFS_FILESYSTEM_TMPFS_SIZE` tunes runtime tmpfs; `SPFS_KEEP_RUNTIME`, `SPFS_RUNTIME_NAME` control durability; `SPFS_METRICS_SYNC_TIME_SECS` recorded when launching.
- **Spec files**: Must live on local filesystem accessible to CLI (remote paths not supported). Run-specs require `api: spfs/v0/runspec`; live layers require `api: spfs/v0/livelayer`. **Platform note**: Spec-file parsing is OS-agnostic; mounting behavior differs per platform (see Platform-specific behavior).
- **Security**: Live layer binds restricted to files under spec directory to prevent escaping into arbitrary host paths.

## Platform-specific behavior
- **Linux**: Live layers use the `mount` command or Linux-specific syscalls (`open_tree`, `move_mount`). Spec-file parsing is OS-agnostic; only mounting differs.
- **Windows**: Live layers are handled by the `spfs-winfsp` service; spec files are passed as part of the environment spec. The same YAML parsing applies, but mounting relies on WinFsp.
- **macOS**: Live layers are handled by the FUSE service; spec-file parsing is identical but mounting is FUSE-based. The `spfs-fuse` service manages the mount points.

## Testing & quality
- **Unit tests**: `tracking/env_test.rs` covers parsing edge cases (empty spec, directories, run-spec validation). `runtime/live_layer_test.rs` validates bind mount paths.
- **Docs as contract**: `docs/spfs/usage.md` clearly enumerates YAML structures; update tests when formats change.
- **CI**: Standard `cargo test`, `make test CRATES=spfs` before shipping new spec features to ensure compatibility across backends.

## Extension points & customization
- **New spec file types**: Add enum variants to `SpecApiVersion` and extend `SpecFile::from_yaml` to parse new `api` values (ensure docs + tests updated).
- **Custom bindings**: Extend `LiveLayerContents` to support future content types (e.g., tmpfs overlays) while maintaining validation rules.
- **CLI behavior**: `cmd_run.rs` is the chokepoint for hooking new spec semantics (e.g., new `--annotation` sources, extra validation).

## Operational notes
- Absolute paths are mandatory for `.spfs.yaml` references so parser can disambiguate them from tags (documented error message).
- Duplicate spec files are rejected to avoid infinite recursion; users should de-duplicate run spec includes.
- When overlay backend requires local renders, CLI resolves tags to digests before syncing to keep local tag namespace clean (`with_tag_items_resolved_to_digest_items`).
- **Platform mounting**: Live layer mounting differs per platform: Linux uses `mount`/syscalls, Windows uses WinFsp service, macOS uses FUSE service.
- Live layer files are applied after overlay stack mounts, so they only mask/override files rather than contributing to stored layers.

## LLM working set
1. `docs/spfs/usage.md` – user-facing description of run-spec & live-layer YAML.
2. `crates/spfs/src/tracking/env.rs` – parser, flattening logic, recursion guard.
3. `crates/spfs/src/tracking/env_test.rs` – sample test data for spec files.
4. `crates/spfs/src/runtime/live_layer.rs` – data model + validation for bind mounts.
5. `crates/spfs/src/runtime/live_layer_test.rs` – edge cases for bind mounts and parent paths.
6. `crates/spfs-cli/main/src/cmd_run.rs` – CLI integration and runtime creation flow.
7. `docs/admin/config.md` – config locations/env vars users may tweak when relying on run specs.

## Open questions
- Are there additional spec APIs (e.g., future `spfs/v1/*`) under development? **Current status**: No new spec APIs have been added since document creation; spec-file parsing code is stable and unchanged.
- How do Windows runtimes treat live layer files (WinFsp backend)? **Current status**: Windows live layers are handled by the `spfs-winfsp` service; parity with Linux mounting behavior is still under investigation.
- Should run-spec files allow relative includes? **Current status**: Relative includes are still not allowed; absolute paths remain required for security.
- **Spec-file parsing stability**: The core spec-file parsing logic in `crates/spfs/src/tracking/env.rs` has remained stable and unchanged since document creation.
