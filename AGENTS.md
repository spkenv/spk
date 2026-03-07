## Cursor Cloud specific instructions

### Project overview

SPK is a package manager built on SPFS (a per-process layered filesystem). Both are written in Rust (edition 2024, toolchain 1.93.0) as a Cargo workspace with ~30+ crates under `crates/`. See `README.md` for details.

### Build, lint, test

Standard commands are in the root `Makefile`:

- **Build:** `make debug FEATURES=server,spfs/server`
- **Clippy:** `make lint-clippy FEATURES=server,spfs/server`
- **Format check:** `make lint-fmt` (requires `cargo +nightly fmt`)
- **Doc lint:** `make lint-docs FEATURES=server,spfs/server`
- **Test:** `make test FEATURES=server,spfs/server` (wraps tests in `spfs run`)
- **Scoped:** add `CRATES=crate1,crate2` to target specific crates

### Running services

No external databases or services are needed. SPFS uses file-based storage.

Before running tests, set these environment variables:

```sh
export SPFS_MONITOR_DISABLE_CNPROC=1
export SPFS_REMOTE_origin_ADDRESS="file:///tmp/spfs-repos/origin"
export SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING=1
```

To initialize the test origin repo (idempotent):

```sh
SPFS_REMOTE_origin_ADDRESS="file:///tmp/spfs-repos/origin?create=true" spfs ls-tags -r origin
```

### Non-obvious caveats

- The `make test` target runs `spfs run - -- cargo test ...`, meaning tests execute inside an SPFS runtime with `/spfs` mounted. SPFS debug binaries must be installed with capabilities set (`make install-debug-spfs`) before tests will work.
- `SPFS_MONITOR_DISABLE_CNPROC=1` is required in containers (no cnproc support).
- `echo user_allow_other >> /etc/fuse.conf` is needed for the FUSE backend.
- The `lint-fmt` target requires Rust nightly: `cargo +nightly fmt --check`.
- Not all crates support the `server` feature; when testing a single crate, omit `FEATURES` if unrelated (e.g. `spfs run - -- cargo test -p spfs-encoding`).
- `flatc` v23.5.26 is required at build time (FlatBuffers schema compilation).
- `ast-grep` is required at build time (installed via `cargo install --locked ast-grep`).
