---
title: Bootstrapping
summary: Initial setup for fresh installations
weight: 20
---

For fresh installs, there will be no existing spk packages that you can use.

## Option 1: Python package via virtualenv

Often, the most important package that folks need access to is a python package. If python is installed on your workstation already, you can use the `venv` module to easily generate an spk package for it.

```yaml
api: v0/package
pkg: python/<version> # replace with the version you have installed, eg 3.12.3
build:
  script:
    python -m venv --copies /spfs
install:
  requirements:
    - pkg: stdfs
```

Save this package to a file named `python.spk.yaml` and run `spk build python.spk.yaml` to generate it. In order to use this package you will need to also build the `stdfs` package, which you can find on our [github](https://github.com/spkenv/spk/blob/main/packages/stdfs/stdfs.spk.yaml).

You can then use `spk env python` to jump into an environment that uses this new local package.

> [WARNING!]
> this python package can be used for running python code and scripts but should not be used for building compiled modules against. For that, you can use the python package recipe that we have in our github repo, which properly describes compatibility for these cases.

## Option 2: Build from source

Our codebase includes a number of package spec files that are both great examples and can be used to bootstrap new environments. In general, the bootstrap process creates proxy packages for software that's installed on the local machine and then uses those to create actual spk packages that can be published and used elsewhere.


```sh
# this command will bootstrap and then build up a
# dependency tree of packages in order to get to a native python build
make packages.python3
```

This command may fail if you don't have the necessary dependencies installed on the local machine. Errors such as `automake is not installed` can be resolved my installing that package via the system package manager, eg: `sudo dnf install automake` on el9.
