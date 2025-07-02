<!-- Copyright (c) Contributors to the SPK project. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- https://github.com/spkenv/spk -->


<img width="200px"
alt="SPK Logo" src="website/static/images/spk_black.png#gh-light-mode-only"/>
<img width="200px"
alt="SPK Logo" src="website/static/images/spk_white.png#gh-dark-mode-only"/>

[![Docs Badge](https://img.shields.io/badge/docs-passing-green.svg)](https://spkenv.dev)
[![OpenSSF Best Practices](https://www.bestpractices.dev/projects/9740/badge)](https://www.bestpractices.dev/projects/9740)

- **SPK** - A Package Manager for high-velocity software environments, built on SPFS.
- **SPFS** - Filesystem isolation, capture, and distribution.

---


## Motivation

SPFS provides a powerful set of tools for capturing and isolating process filesystems at runtime, but not a lot of great workflows for managing and organizing the environments and layers. SPK is the solution to this problem, adding the concept of software packages and the process of environment and dependency resolution for a better workflow.

## Usage

See the main [docs](docs/) for details on using spk, starting with the [index](docs/_index.md).

## License

SPK/SPFS are Copyright (c) Contributors to the SPK project.
All Rights Reserved.

SPK/SPFS are distributed using the [Apache-2.0 license](LICENSE).

## Structure of this project

`spfs` is the per-process layered file system.

`spk` is the software packaging system built on top of SPFS.

The `packages` directory contains a collection of recipes that can be used as examples or to build out common open source software packages.

## Contributing

Please read [Contributing to SPK](CONTRIBUTING.md).

## Development

Both SPK and SPFS are written in Rust and use cargo. The best way to get started with Rust development is to install the latest stable Rust toolchain using [rustup](https://rustup.rs).

For details on architecture and design of the codebase, see the [developer docs](docs/develop).

```sh
# once cargo is installed, you can build and install both projects with
make build

# additionally features can be activated in all relevant cargo commands as desired
#   protobuf-src will
make build FEATURES=spfs/protobuf-src
```

### Binaries and Capabilities

SPFS builds into a number of separate binaries, all of which can be run through the main `spfs` binary. Some of these binaries require special capabilities to be set in order to function properly. The `setcap` Makefile target can be used to set these capabilities on your locally-compiled debug binaries.

```sh
# assign the necessary capabilities to the debug binaries
make setcap bindir=$PWD/target/debug

# alternatively, assign the capabilities and install the debug binaries
make install
```

### RPM Package

The codebase is set up to produce a centos7-compatible rpm package for both spfs and spk by building them in a docker container. To create the rpm package, you will need docker installed. These packages are also built and made available in this repository's CI.

```sh
# build the rpm package via docker and copy into ./dist/rpm
make rpms
```

### Testing

Both projects have a number of unit and integration tests as well as testable examples that can all be executed with `make test`. The tests for spk need to be executed under an SPFS runtime in order to properly execute. We also have configured linting rules that must pass for all contributions.

Our repository is broken down into a number of smaller crates for easier development, and they can be individually targeted in the makefile which can greatly reduce the time it takes for testing and linting.

```sh
# run the unit test suite
make test
# check the code for lint
make lint

# only lint and test two specific crates
make lint test CRATES=spfs-encoding,spfs-cli-common
```

### Bootstrapping

> [!WARNING]
> The existing bootstrapping setup was developed for CentOS 7 and may need
> updates as we transition to el9 distributions.

In a new environment, it can be helpful to build all of the core packages whose recipes ship with SPK. A script has been provided which runs through all of the builds for these packages in the right order.

```sh
# bootstrap and build all core packages (takes a long time)
make packages
# build only enough to bootstrap a compiler and linker
make packages.bootstrap
# build the python2 package (will require at least packages.bootstrap)
make packages.python2
```

Some of these package specs have not yet been used or tested fully or ironed out all the way so please communicate any issues as you run into them!

#### Using Docker

> [!WARNING]
> The this process was setup to create packages in CentOS 7 and may need
> updates as we transition to el9 distributions.

Currently, this process can only be run on an rpm-based system, as it relies on some rpm packages being installed on the host in order to bootstrap the build process. If you are not running on an rpm-based system, you can run the process in a container instead:

```sh
# build bootstrap packages in a docker image
# (can also build any other packages.* rule, though the container startup is heavy)
make packages.docker.python2
# build all core packages
make packages.docker
# import the created packages to the local spk environment
make packages.import
```

#### Conversion Packages

Spk has logic to automatically convert pip packages to spk packages for easy Python environment creation. This logic lives and runs inside of its own spk package/environment. If you have python3 already installed, you can generate this package locally like so:

```sh
make converters
```

Once built, these packages will need to be published in order to use them from the `spk convert` command.

```sh
make converters
spk publish spk-convert-pip/1.0.0
spk convert pip --help
```

#### Other Notes

- The `make packages.python3` target can be used to bootstrap just enough to be able to build Python for SPK. The Python recipes will build multiple Python versions for each gcc48 and 63 as well as for the different Python ABIs
- The make `packages.gnu` target can be used to bootstrap just enough to get "native" spk packages for gcc48 and gcc63

Of course, the packages themselves can also be built with the `spk build <spec_file>` command directly, though you may find that some required build dependencies need to be generated with the `make packages.bootstrap.full` command first.

The following RPM packages must be installed in order to create the bootstrap packages.

```bash
sudo yum install -y \
    autoconf \
    autoconf-archive \
    autogen \
    automake \
    binutils \
    bison \
    coreutils \
    flex \
    gcc \
    gettext \
    glibc \
    grep \
    help2man \
    libtool \
    m4 \
    make \
    perl \
    sed \
    texinfo \
    zip \
    zlib
```

SPFS has a number of unit tests written in Rust that can be run using the `cargo` command or via our make target.

```sh
make test
```

Additionally, there are a number of integration tests that validate the fully installed state of spfs. These are generally a series of SPFS command line calls that validate the creation and usage of the `/spfs` filesystem.

```sh
cargo build
make setcap bindir=$PWD/target/debug
tests/integration/run_all.sh
```

### Windows

To build and run on Windows, you will first need to use the Windows package manager `winget` to install
Chocolatey.

```sh
winget install Chocolatey
```

Next you need a couple of dependencies that are easiest to install via Chocolatey.

```sh
choco install make protoc llvm winfsp
```

Next we need to install the FlatBuffers compiler. You will need to run PowerShell as Administrator.

```powershell
$url = "https://github.com/google/flatbuffers/releases/download/v23.5.26/Windows.flatc.binary.zip"
Invoke-WebRequest $url -OutFile "$Env:TEMP\flatc.zip"
New-Item -ItemType Directory -Force -Path "C:\Program Files\Google\flatc"
Expand-Archive "$Env:TEMP\flatc.zip" -DestinationPath "C:\Program Files\Google\flatc"
Remove-Item -Path "$Env:TEMP\flatc.zip"
[Environment]::SetEnvironmentVariable(
    "Path",
    [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::Machine) + ";C:\Program Files\Google\flatc",
    [EnvironmentVariableTarget]::Machine)
```

### Benchmarks

Benchmark tests can be found in `benches/`. All benchmark tests can be run with `cargo bench`, but in order to successfully pass `criterion`-specific options to the `criterion`-based benchmarks, those types of benchmarks need to be filtered for.

```sh
cargo bench --bench spfs_bench
```

A common workflow as described [here](https://bheisler.github.io/criterion.rs/book/user_guide/command_line_options.html#baselines) is to record a baseline measurement to use as a reference to compare future measurements against.

```sh
git checkout main
# Record baseline with name "main"
cargo bench --bench spfs_bench -- --save-baseline main

git checkout topic-branch
# While iterating, this creates a new baseline called "new", and
# will report on the change since the most recent "new".
cargo bench --bench spfs_bench

# Compare to "main"
cargo bench --bench spfs_bench -- --load-baseline new --baseline main
```
