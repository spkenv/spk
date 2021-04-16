# SPK

Package Manager for SPFS.

## Motivation

SpFS provides a powerful set of tools for capturing and isolating process filesystems at runtime, but not a lot of great workflows for managing and organizing the environments and layers. SPK is the solution to this problem, adding the concept of software packages and the process of environment and dependency resolution for a better workflow.


## License

SPK/SPFS/spawn are Copyright (c) 2021 Sony Pictures Imageworks, et al.
All Rights Reserved.

SPK/SPFS/spawn are distributed using the [Apache-2.0 license](LICENSE.txt).


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
