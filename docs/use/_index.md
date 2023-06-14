---
title: Usage
summary: User documentation for spk
weight: 10
---

The `spk` command line has a great number of useful commands to explore, simply run `spk --help` to explore.

### Run an Environment

```bash
# enter a shell environment with an existing package installed
$ spk env python/2
$ which python
/spfs/bin/python

# or run a command directly
$ spk env python/2 -- python
```

Check the [Version Semantics](versioning) for help on how to request packages.

### Create a Package

```bash
# generate a basic spec file to get started
$ spk new my-pkg

# make any necessary changes to the file and then build it
$ spk build

# run environments using locally built packages
$ spk env --local my-pkg
```

Use the [Package Definition Guide](spec) for more details.
Check the included [examples](https://github.com/imageworks/spk/tree/main/examples) for additional help.

For more detailed information on the build process, check the [Package Build Process](build)

### Publish a Package

```bash
# publish a locally built package for others to use
$ spk publish my-pkg/0.1.0
```
