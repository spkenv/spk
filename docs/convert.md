---
title: External Packages
summary: Convert packages from other package managers for use in spk.
---

The `spk convert` command can be used to ingest packages from supported package managers into spk. One converted, these packages are available for local testing and can also be published for others to use.


## SpComp2

SpComp2 libraries can be converted. This process will also recursively find and convert any dependencies of the requested spComp2.

```sh
# convert the current version of filesequence
$ spk convert spcomp2 FileSequence
# or request a specific version
$ spk convert spcomp2 FileSequence/v6
```

When being converted, the spComp2 libraries, and headers are copied into `/spfs` under `lib/` and `include/`, respectively. Additionally, the process strips all RPATHs from the binaries so that they pick up their dependencies.
