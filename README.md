<!-- Copyright (c) 2021 Sony Pictures Imageworks, et al. -->
<!-- SPDX-License-Identifier: Apache-2.0 -->
<!-- https://github.com/imageworks/spk -->

# SPK

Package Manager for SPFS.

## Motivation

SpFS provides a powerful set of tools for capturing and isolating process filesystems at runtime, but not a lot of great workflows for managing and organizing the environments and layers. SPK is the solution to this problem, adding the concept of software packages and the process of environment and dependency resolution for a better workflow.


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

As an spdev project, you can build and validate your local clone of spk by entering the development environment and running the whole workflow:

```sh
spdev env
spdev flow
```

When making changes to the rust portion of the codebase, it may need to be rebuilt in order to test locally:

```sh
python setup.py develop
```
