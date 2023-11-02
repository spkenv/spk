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

Check the [Version Semantics]({{< ref "use/versioning" >}}) for help on how to request packages.

### Create a Package

```bash
# generate a basic spec file to get started
$ spk new my-pkg

# make any necessary changes to the file and then build it
$ spk build

# run environments using locally built packages
$ spk env --local my-pkg
```

Use the [Package Definition Guide]({{< ref "use/spec" >}}) for more details.
Check the included [examples](https://github.com/imageworks/spk/tree/main/examples) for additional help.

For more detailed information on the build process, check the [Package Build Process]({{< ref "use/build" >}})

### Publish a Package

```bash
# publish a locally built package for others to use
$ spk publish my-pkg/0.1.0
```

### Run an Environment In The Past

For debugging and recovery workflows, the `--when` flag can be provided to run spk commands
as they would have done at some relative or absolute time in the past (see `spk env --help` for more details).

```bash
# enter a shell environment using the repository state from 10 minutes ago
$ spk env python/2 --when ~10m
$ which python
/spfs/bin/python

# or run a command directly
$ spk env python/2 --when ~10m -- python
```
