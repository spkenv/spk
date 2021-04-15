# SPK

Package Manager for SPFS.

## Motivation

SpFS provides a powerful set of tools for capturing and isolating process filesystems at runtime, but not a lot of great workflows for managing and organizing the environments and layers. SPK is the solution to this problem, adding the concept of software packages and the process of environment and dependency resolution for a better workflow.

## Development

SPK uses Pipenv to define it's development and build environment. It also requires a working Rust compiler and the Cargo toolchain for building a native Python extension.

### Getting Set Up

To get into the development environment, start by activating the python virtualenv with Pipenv:

```sh
# enter the python virtualenv for development
pipenv shell --dev
```

Then, build and install the spk package in development mode using python setuptools:

```sh
# install the local sources into the virtualenv
python setup.py develop
```

From this shell, you can run the local build of the `spk` command as well as all tests with pytest. **NOTE**: running the local development version of spk, and running the unit tests will require that `spfs` is installed on the local machine. The pytest test suite must also be run from within an spfs environment in order to work properly.

```sh
# run the unit test suite
spfs run - -- pytest
```

### Bootstrapping

In a new environment, it can be helpful to build all of the core packages who's recipes ship with SPK. A script has been provided which runs through all of the builds for these packages in the right order.

```sh
bash packages/build_all.sh
```
