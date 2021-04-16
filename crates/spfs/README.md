<!-- Copyright (c) 2021 Sony Pictures Imageworks, et al. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- https://github.com/imageworks/spk -->

# spfs

Filesystem isolation, capture, and distribution.

Additional information is available under [docs](docs/).


## License

SPK/SPFS/spawn are Copyright (c) 2021 Sony Pictures Imageworks, et al.
All Rights Reserved.

SPK/SPFS/spawn are distributed using the [Apache-2.0 license](LICENSE.txt).


## Development

SpFS is written in Rust and uses Cargo. The best way to get started with rust development is to install the latest stable rust toolchain using [rustup](https://rustup.sh). More detailed design docs are available under [docs/design](docs/design/).

### Building

Once setup with Rust, building and running a local debug build of spfs is as easy as:

```sh
cargo build
target/debug/spfs --help
```

### Binaries and Capabilities

Spfs builds into a number of separate binaries, all of which can be run through the main `spfs` binary. Some of these binaries require special capabilities to be set in order to function properly. The `setcaps_debug.sh` script can be used to set these capabilities on your locally-compiled debug binaries.

```sh
sudo setcaps_debug.sh
```

### RPM Package

The spfs codebase is setup to produce a centos7-compatible rpm package by building spfs in a docker container. To create the rpm package, you will need docker installed. These packages are also built and made available in this repository's CI.

```sh
# build the rpm package via docker and copy into ./dist/rpm
make rpm
```

### Testing

Spfs has a number of unit tests written in rust that can be run using the `cargo` command.

```sh
cargo test
```

Additionally, there are a number of integration tests that validate the fully installed state of spfs. These are generally a series of spfs command line calls that validate the creation and usage of the `/spfs` filesystem.

```sh
cargo build
./setcaps_debug.sh
tests/integration/run_all.sh
```
