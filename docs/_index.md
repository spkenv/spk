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

Check the [Version Semantics](versioning.md) for help on how to request packages.

### Create a Package

```bash
# generate a basic spec file to get started
spk new my_pkg

# make any necessary changes to the file and then build it
spk build my_pkg.yaml
```

Use the [Package Definition Guide](spec.md) for more details.
