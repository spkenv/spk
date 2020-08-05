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

## Pip

Pip packages from pypi can be converted into spk packages as well. This process will recursively find and convert any dependencies of the requested pip package as well.

```sh
# convert the current version of filesequence
$ spk convert pip gitlab-python
# or request a specific version (using pip version semantics)
$ spk convert spcomp2 gitlab-python==1.7

$ spk env gitlab-python --local -- python -c "import gitlab; print(gitlab)"
```

As of writing, the converted package will also be dependant on the current os and arch since noarch support cannot easily be detected.

```sh
# convert for a specific python version
spk convert pip --python-version 2.7 numpy
```
