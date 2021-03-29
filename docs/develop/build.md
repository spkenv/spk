---
title: Build and Testing
summary: Information about building and testing the spk codebase
weight: 50
---


The spk codebase is largely written in python, but includes a python extension written in rust. This extension exists so that spk can interact directly with the spfs api, which is also written in rust. At the time of writing, only this interaction is in rust but it is a reasonable goal to start moving some portions of the spk codebase into rust to improve performance. Eventually, it would be nice to have spk entirely implemented in rust.

The spk codebase is setup for use with `spdev`, making it easy get started.

### First Build

The `dev env` command will place you into a shell that's properly configured for working with the spk codebase. Once in this shell, `dev flow` will build and package the entire codebase from scratch.

### Dev Loop

The `dev build` and `dev test` commands can be used independantly to build and test the codebase, but the `spk` and `pytest` commands can also be invoked separately for local execution and testing, respectively.

Before running spk as a local command, it will need to be installed into the virtual envrionment in development mode. From within the `dev env` shell, run `python setup.py develop` to accomplish this. You may also need to execute this command any time that you change the rust portion of the spk codebase, to trigger a rebuild of the extension.

### Unit Testing

Spawn uses pytest for all of it's unit testing, using `*_test.py` files directly next to the files that are being tested within the main package source tree. For example, the `spk/_global.py` module is tested in the `spk/_global_test.py` file. Please maintain this convention throughout.
