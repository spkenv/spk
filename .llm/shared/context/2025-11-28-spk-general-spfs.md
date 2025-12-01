---
date: 2025-11-29T00:00:00-08:00
repository: spk
git_commit: d2855f085f4ebaa50a2241c013c88c3187cd6076
branch: main
discovery_prompt: "/dicovery update existing discovery please"
generated_by: "opencode:/discovery"
tags: [context, discovery]
status: complete
last_updated: 2025-11-29
---

# Repo Context Guide: spk

## TL;DR
- Monorepo for SPK (package manager) and SPFS (per-process layered filesystem) written in Rust.
- Split into many crates: SPFS runtime/storage/tracking, SPK CLI & solver stacks, helpers (macros, encoding, VFS, workspace tools).
- Ships large catalog of package specs under `packages/` plus docs, website, and examples for onboarding.
- Builds via `cargo` + Makefile orchestration; packaging tooling can emit RPMs and self-contained SPK platforms.
- Focus: how SPFS layers underpin SPK workflows; how SPK CLI, solver, and builder orchestrate repos, specs, and runtimes.

## Quickstart (dev)
- **Prereqs**: Latest stable Rust via `rustup`, `make`, `protoc`, FlatBuffers `flatc`, Docker (for RPM builds), Linux capabilities (`setcap`) or WinFsp/LLVM on Windows (see `README.md`).
- **Setup**:
  1. Clone repo, run `rustup default stable`.
  2. Install FlatBuffers, ast-grep (`cargo install --locked ast-grep`), and OS-specific deps (Win: `choco install make protoc llvm winfsp`).
  3. Initialize SPFS repo structure (`mkdir -p <repo>/{objects,payloads,tags}`) or via `spfs repo init`.
- **Build/Run**:
  - `make build` builds all workspace crates (SPFS + SPK).
  - `cargo run -p spfs -- shell -` boots empty `/spfs` shell; `spk env <pkg>` resolves packages.
  - Grant caps: `make setcap bindir=$PWD/target/debug` (Linux) for namespace/mount binaries.
- **Test**:
  - `make test` for unit/integration; scoped via `make lint test CRATES=spfs,spk-cli`.
  - Integration runtime tests under `crates/spfs/tests/integration` (require privileges); `cargo bench --bench spfs_bench` for perf.
- **Common commands**:
  - `make lint`, `make packages`, `make packages.bootstrap`.
  - `spfs shell <refs> [--edit]`, `spfs run <refs> -- <cmd>`, `spfs commit layer|platform --tag <name>`.
  - `spk new <pkg>`, `spk build <spec>`, `spk env <pkg[/vers]>`, `spk publish <pkg/vers>`.

## How to use (user)
- **SPFS basics**: `spfs shell -` → isolated `/spfs`; commit snapshots via `spfs commit layer --tag dev/base`; stack into platforms with `spfs commit platform --tag dev/platform`.
- **SPFS workflows**: use tag streams (`spfs log <tag>`, `spfs tag my-tag~2 my-tag`) and live layers by passing `layer.spfs.yaml` to `spfs run` (see `docs/spfs/usage.md`).
- **SPK CLI**:
  - Author specs (`spk new my-pkg`, edit YAML under `packages/`), build (`spk build my-pkg.spk.yaml`), test (`spk test`), publish (`spk publish my-pkg/1.0.0`).
  - Resolve environments: `spk env platform/pkg -- python`, `spk env pkg --when ~10m` for historical state (`docs/use/command.md`).
  - Create platforms via `api: v1/platform` specs to capture dependency opinions (`docs/use/platforms.md`).

## Repo map
- `crates/spfs/`: core filesystem library (runtime, storage backends, tracking, sync, CLI helpers like `bootstrap.rs`).
- `crates/spfs-cli/*`: CLI groups & subcommands (`cmd-run`, `cmd-clean`, `cmd-fuse`, etc.).
- `crates/spk/` & `crates/spk-cli/*`: SPK library/CLI entry (command routing, group commands, build/render/test cmd crates).
- `crates/spk-build`, `spk-solve`, `spk-storage`, `spk-workspace`: builder, solver, repository abstractions, workspace parsing.
- `crates/spk-config`, `spk-exec`, `spk-launcher`: config loading, exec helpers, launcher for versioned binaries.
- `crates/spfs-encoding`, `spfs-proto`, `spfs-vfs`: shared encoders, proto schema, FUSE/WinFsp VFS.
- `packages/`: canonical package recipes (YAML). `docs/`: Hugo docs for users/admins/devs. `examples/`: cmake/python/spec samples. `website/`: site theme/content.
- **Read this first**: `README.md`, `docs/spfs/_index.md`, `docs/use/command.md`, `docs/use/create/spec.md`, `crates/spfs/src/lib.rs`, `crates/spk-cli/cmd-build/src/cmd_build.rs`, `crates/spk-workspace/src/workspace.rs`.

## Architecture overview
- **SPFS object graph**: DAG of platform → layers → manifests → trees → blobs (`docs/spfs/develop/design.md`). Objects encoded via `spfs-encoding`, hashed SHA256, stored in repositories.
- **Storage layer**: repository traits (`crates/spfs/src/storage/mod.rs`) with implementations (filesystem, tar, proxy, pinned, rpc). Renderers materialize manifests for overlay or FUSE.
- **Runtime layer**: `crates/spfs/src/runtime` orchestrates resolving refs, rendering, namespace/mount setup (overlayfs, fuse, winfsp), startup scripts, durability.
- **Tracking layer**: `crates/spfs/src/tracking` captures diffs, tag streams, manifests for commits.
- **SPK pipeline**:
  - **Specs & workspace**: `spk-schema` defines spec formats; `spk-workspace` loads template sets and resolves `api: v0/package` or `v1/platform` definitions.
  - **Solver**: `spk-solve` resolves dependency graphs, compatibility, variant enumeration (metrics integration).
  - **Builder**: `spk-build` + CLI orchestrate make-source + make-binary (source capture under `/spfs/spk/pkg/<name>/<ver>/src` then install stage).
  - **Storage**: `spk-storage` wraps repository handles for packages (build artifacts, disk-usage reporting used in `cmd_build.rs`).
  - **CLI**: layered command groups (bake/deprecate, ls/new/publish, export/import, lint/search/view) under `spk-cli/group*` plus `cmd-*` crates for heavy commands.
- **Docs & site**: Hugo content in `docs/` (user/admin/dev) mirrored to `website/` for spkenv.dev.

## Key components (deep links)
### SPFS runtime (`crates/spfs/src/runtime/*`)
- **Purpose**: Manage runtime configs (`storage.rs`), mounting (`env.rs`), startup scripts (`startup_*.rs`), status transitions (`status_unix.rs`, `status_win.rs`).
- **Entry points**: `spfs::status::initialize_runtime`, `Runtime::save_state_to_storage`, CLI `spfs run` (`crates/spfs-cli/main/src/cmd_run.rs`).
- **Invariants**: Stack order preserved bottom-up; overlay/fuse backends recorded in runtime storage; durable runtimes move upper/work dirs into repo-managed locations.

### SPFS storage (`crates/spfs/src/storage`) 
- **Purpose**: Persist DAG objects, payloads, tags; render manifests. Key modules `fs/`, `tar/`, `proxy/`, `fallback/`, `rpc/`.
- **Highlights**: `fs/renderer.rs` hard-links payloads when rendering; `repository.rs` enforces object layout; migrations under `migrations/`.

### SPFS tracking (`crates/spfs/src/tracking`)
- **Purpose**: Build manifests from active filesystem, diff snapshots, manage tag streams. `diff.rs`, `manifest.rs`, `env.rs` for capture.
- **Usage**: `spfs commit layer|platform` flows rely on capturing active edits before creating tagged objects.

### SPK CLI dispatcher (`crates/spk/src/cli.rs`)
- **Purpose**: Clap-based entrypoint wiring subcommands to `Run` trait, with logging, Sentry/statsd hooks.
- **Notables**: Subcommands implemented across `spk-cli` crate tree; `configure_logging` must run before command exec; metrics reported via `spk-solve`.

### `spk-cli/cmd-build` pipeline (`crates/spk-cli/cmd-build/src/cmd_build.rs`)
- **Purpose**: Execute make-source + make-binary per package set, optionally interactive or `--here` builds, compute disk usage via `spk_storage`.
- **Flow**: Ensures SPFS runtime active, splits packages, appends source ident to ensure binary ties to source, prints per-build/per-package disk stats.

### Workspace tooling (`crates/spk-workspace/src/workspace.rs`)
- **Purpose**: Load and query template sets, find specs by name/version/path, support overriding configs and multiple templates per package.
- **Key behaviors**: Handles `LowestSpecifiedRange` for `pkg/major` queries, canonicalizes file paths, merges template configs.

### Docs & examples
- `docs/use/*`: user workflows (build/publish, convert, solver). `docs/use/create/*.md` explains spec fields, sources, tests.
- `docs/spfs/usage.md`: advanced SPFS commands (tag streams, run specs, live layers).
- `examples/`: sample spec trees, cmake/python integration, testing scenario.

## Configuration & environments
- **SPFS config**: `spfs::config::get_config()` loads from `$SPFS_CONFIG` or `~/.config/spfs/config.toml`. Keys include `[filesystem] backend`, `secondary_repositories`, `[fuse]` heartbeat/mount settings. Env vars: `SPFS_FILESYSTEM_TMPFS_SIZE`, `SPFS_DIR_PREFIX`, `SPFS_RUNTIME`, `SPFS_SHELL_MESSAGE`.
- **SPK config**: `.spdev` overlays (`spi/.spdev/`), workspace files (`workspace.spk.yaml`, `workspace.spk.yml`) list templates/build graph. `spk-launcher` honors `$SPK_BIN_TAG`/`$SPK_BIN_PATH` to select platform-delivered binaries.
- **Secrets**: Remotes may embed credentials in URLs; repo contains no secrets. Publishing typically requires authenticated remotes configured outside repo.

## Testing & quality
- **Unit**: Each crate includes `*_test.rs` modules (e.g., `runtime/storage_test.rs`, `spk-cli/group*/cmd_*_test.rs`).
- **Integration**: `tests/integration/` for SPFS overlay/fuse flows; builder tests under `spk-cli/cmd-build/src/cmd_build_test`. Many require elevated capabilities or prepared repos.
- **CI**: `.github/workflows/*.yml` run lint, tests, coverage, rpm builds, cspell. `codecov.yml`, `.clippy.toml`, `.rustfmt.toml`, `.taplo.toml` enforce formatting/lints; `cspell.json` for spell-check.
- **Benchmarks**: `crates/spfs/benches`, run via `cargo bench --bench spfs_bench` with baseline workflow (documented in `README.md`).

## Extension points & customization
- **Storage backends**: Implement `storage::Repository` interfaces and wire via config/CLI to support new persistence layers.
- **Runtime**: Add new `MountBackend` variants (e.g., alternative virtualization) plus startup scripts.
- **SPK commands**: Add new subcommands under `spk-cli/cmd-*` crates and register inside `crates/spk/src/cli.rs`.
- **Specs/platforms**: Author new templates in `packages/`; use inheritance via `api: v1/platform` to enforce org-specific constraints (`docs/use/platforms.md`).
- **Workspace automation**: Extend `spk-workspace` builder to pre-load template directories, or feed custom `TemplateConfig` for multi-version builds.

## Operational notes
- **Deployment**: Build/install via `make install`; RPM packages via `make rpms` (CentOS7 base) or `make packages.docker`. `spfs-enter`/`spfs-fuse` binaries need `CAP_SYS_ADMIN`.
- **SPK launcher**: `spk-launcher` binary can switch between installed versions using SPFS platforms stored under tags like `spk/spk-launcher/<tag>`.
- **Observability**: Optional Sentry integration (`spk_cli_common::configure_sentry`), statsd metrics for CLI run counts/errors, tracing via `tracing` crate; `spfs` exposes Sentry + `SPFS_METRICS_...` env hooks.
- **Performance**: Overlay render uses hardlinks (fast but inode-heavy). For huge layer stacks, runtime may flatten manifests to avoid kernel arg limits. SPK build disk-usage reporting helps capacity planning.
- **Windows**: Build instructions in `README.md` (winget + choco dependencies, WinFsp). `crates/spfs/src/runtime/winfsp.rs` parallels overlay backend.

## LLM working set
- **Top files to load**:
  1. `README.md` – repo overview, dev workflow, testing.
  2. `docs/spfs/_index.md`, `docs/spfs/develop/design.md`, `docs/spfs/develop/runtime.md` – SPFS concepts.
  3. `docs/use/command.md`, `docs/use/build.md`, `docs/use/create/spec.md`, `docs/use/platforms.md` – SPK user workflows/spec format.
  4. `crates/spfs/src/runtime/storage.rs`, `status_unix.rs`, `env.rs` – runtime internals.
  5. `crates/spfs/src/storage/fs/repository.rs` & `renderer.rs` – object storage/rendering.
  6. `crates/spk/src/cli.rs`, `crates/spk-cli/cmd-build/src/cmd_build.rs` – CLI orchestration.
  7. `crates/spk-workspace/src/workspace.rs` – spec lookup.
  8. `Cargo.toml` – workspace members/dependencies/feature surfaces.
  9. `packages/*.spk.yaml` – concrete spec examples.
  10. `.github/workflows/coverage.yml` (representative of CI expectations).
- **Common Q&A anchors**: `docs/spfs/usage.md` (tag streams/live layers), `docs/use/create/` (spec fields), `spk-launcher/README.md` (launcher behavior), `Makefile` + `Makefile.linux` (targets/capabilities), `spfs.spec` & `spk.spec` (packaging requirements).
- **Glossary**:
  - **Layer**: Immutable filesystem snapshot identified by digest/tag.
  - **Platform**: Ordered stack of layers or package requirements describing an environment.
  - **Live layer**: Runtime bind-mount overlay injecting host paths via YAML descriptor.
  - **Runspec**: YAML list of refs consumed by `spfs run` to shorten CLI invocations.
  - **Workspace**: Collection of package templates/specs for building multiple packages cohesively.
  - **Variant**: Specific build option set resolved by solver (architecture, compiler, etc.).

## Open questions
- Windows parity: confirm latest WinFsp backend features (`crates/spfs/src/runtime/winfsp.rs`, docs covering win workflows) remain in sync with Linux overlay/fuse paths.
- Remote repository bootstrap: need canonical procedure beyond manual directory creation (check `docs/admin/*` for automated tooling or scripts).
- SPK solver telemetry: investigate how statsd metrics map to repo operations (`spk-solve` docs/code) and whether defaults require extra config.
- Packaging recipes: ensure `packages/` inventory is up to date with el9 bootstrap requirements noted in `README.md` warnings.
- FUSE heartbeat + monitor interplay: confirm operational defaults (`docs/spfs/develop/runtime.md` vs `crates/spfs/src/monitor.rs`) for large deployments.
