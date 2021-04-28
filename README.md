<!-- Copyright (c) 2021 Sony Pictures Imageworks, et al. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- https://github.com/imageworks/spk -->

# SPK

Package Manager for SPFS.

## Motivation

SpFS provides a powerful set of tools for capturing and isolating process filesystems at runtime, but not a lot of great workflows for managing and organizing the environments and layers. SPK is the solution to this problem, adding the concept of software packages and the process of environment and dependency resolution for a better workflow.

## Usage

See the main [docs](docs/) for details on using spk, starting with the [index](docs/_index.md).

## License

SPK/SPFS/spawn are Copyright (c) 2021 Sony Pictures Imageworks, et al.
All Rights Reserved.

SPK/SPFS/spawn are distributed using the [Apache-2.0 license](LICENSE.txt).


## Structure of the spk project

`spfs` is the per-process layered file system.

`spk` is the software packaging system built on top of SPFS.

`spawn` is the application launcher for spk packages.

These are spread over three code bases at the moment, but will probably
be merged into a single project, [spk](https://github.com/imageworks/spk).
Please refer to [spk](https://github.com/imageworks/spk) for almost all
information about staging the open source project, that's where the
developer documentation and communication will live, including
[Contributing to SPK](https://github.com/imageworks/spk/CONTRIBUTING.md).


## Contributing

Please read [Contributing to SPK](https://github.com/imageworks/spk/CONTRIBUTING.md).


## Development plan

Please read [SPK open source development plan](https://github.com/imageworks/spk/OPEN_SOURCE_PLAN.md).


## Development

SPK is mostly written in python, with a Rust extension that integrates with the spfs API.

For details on architecture and design of the codebase, see the [developer docs](docs/develop).

Python dependencies are tracked with [Pipenv](https://github.com/pypa/pipenv#installation), which will need to be installed. You will also need access to the rust toolchain, which can be installed with [rustup](https://rustup.sh).

Once you have access to the pipenv command, jump into a development environment using:

```sh
pipenv sync --dev
pipenv shell
```

The easiest way to work with spk is to install a local development version into the pipenv virtaul environment. Once in a pipenv shell, this can be achieved by running the commands below.

```sh
# install the local sources into the virtualenv
python setup.py develop
which spk # now points to local dev version
spk --help
```

**NOTE** In order to run spk you will need [spfs](https://github.com/imageworks/spfs) to already be installed on the local system.

### RPM Package

The spk codebase is setup to produce a centos7-compatible rpm package by building spfs in a docker container. To create the rpm package, you will need docker installed. These packages are also built and made available in this repository's CI.

In order to properly build the rpm, you will need to provide your github username and an access token so that the container can pull the spfs sources to build against. The Makefile is setup to prompt you for and fill in these values automatically. If you don't wish to fill these in each time, you can also set the `SPFS_PULL_USERNAME` and `SPFS_PULL_PASSWORD` environment variables before calling make.

```sh
# build the rpm package via docker and copy into ./dist/rpm
make rpm
```

### Testing

Spfs has a number of unit and integration tests as well as testable examples that can all be executed with `pytest`. The tests themselves need to be executed under an spfs runtime in order to properly execute.

```sh
spfs run - -- pytest
```

Additionally, there are some rust unit tests that can be executed using `cargo`.

```sh
cargo test
```

From this shell, you can run the local build of the `spk` command as well as all tests with pytest. **NOTE**: running the local development version of spk, and running the unit tests will require that `spfs` is installed on the local machine. The pytest test suite must also be run from within an spfs environment in order to work properly.

```sh
# run the unit test suite
spfs run - -- pytest
```

### Bootstrapping

In a new environment, it can be helpful to build all of the core packages who's recipes ship with SPK. A script has been provided which runs through all of the builds for these packages in the right order.

```sh
make packages
```

Currently, this process can only be run on an rpm-based system, as it relies on some rpm packages being installed on the host in order to bootstrap the build process.
