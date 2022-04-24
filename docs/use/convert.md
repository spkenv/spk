---
title: External Packages
summary: Convert packages from other package managers for use in spk.
weight: 40
---

The `spk convert` command can be used to ingest packages from supported package managers into spk. One converted, these packages are available for local testing and can also be published for others to use.

## Pip

Pip packages from pypi can be converted into spk packages as well. This process will recursively find and convert any dependencies of the requested pip package as well.

```sh
# convert the current version of filesequence
$ spk convert pip gitlab-python
# or request a specific version (using pip version semantics)
$ spk convert pip gitlab-python==1.7

$ spk env gitlab-python --local -- python -c "import gitlab; print(gitlab)"
```

As of writing, the converted package will also be dependant on the current os and arch since noarch support cannot easily be detected.

```sh
# convert for a specific python version
spk convert pip --python-version 2.7 numpy
```
