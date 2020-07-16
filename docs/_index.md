---
title: spk
summary: Package Manager for SPFS
---

A packaging system and package manager for spfs. Pronouced like "_s-pack_".

### Run an Environment

```bash
# enter a shell envrionment with an existing package installed
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
$ spk build my-pkg.yaml

# run environments using locally built packages
$ spk env --local my-pkg
```

Use the [Package Definition Guide](spec) for more details.
Check the included [examples](https://gitlab.spimageworks.com/dev-group/dev-ops/spk/-/tree/master/examples) for additional help.

For more detailed information on the build process, check the [Package Build Process](build)

### Publish a Package

```bash
# publish a locally built package for others to use
$ spk publish my-pkg/0.1.0
```
