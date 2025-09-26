---
title: Importing Pip Packages
summary: Convert packages from other package managers for use in spk.
weight: 40
---

The `spk convert` command can be used to ingest packages from supported package managers into spk. One converted, these packages are available for local testing and can also be published for others to use.

## Pip

Pip packages from pypi can be converted into spk packages as well. This process will recursively find and convert any dependencies of the requested pip package as well.

```sh
# convert the current version of gitlab-python
$ spk convert pip gitlab-python
# or request a specific version (using pip version semantics)
$ spk convert pip gitlab-python==1.7

# the imported packages will have python- prefixed to their name
$ spk env python-gitlab-python --local -- python -c "import gitlab; print(gitlab)"
```

> [!NOTE]
> The `spk convert pip` command relies on an spk package called `spk-convert-pip`. We provide a recipe for this package in our github repo, which can be used to build and publish it for use in new environments. See [bootstrapping]({{< ref "../../admin/bootstrap" >}}).

The converted package will also be dependant on the current os and arch since noarch support cannot easily be detected.

```sh
# convert for a specific python version, usually you want to override the abi as well
spk convert pip --python-version 3.11 --python-abi=cp311m numpy
```

## Other Package Sources

The `spk convert` base command can be extended to import packages from any other system that you desire. When executing `spk convert <name>`, spk will try to resolve an environment with a package named `spk-convert-<name>` and then inside that it will run a command called `spk-convert-<name>` with any additional arguments passed from the original `spk convert` invocation.

Spk expects that the `spk-convert-<name>` command will do all the work to generate an spk spec file, and build it in the local repository based on whatever semantics make sense for the underlying package source and provided command line arguments.

In this way, you can write your own script or software to ingest or generate packages from any external source that you desire, provided that the necessary information is available and can be mapped into spk. The code and package spec for the pip conversion process is available [in our github](https://github.com/spkenv/spk/tree/main/packages/spk-convert-pip).

> [!TIP]
> Best practice is to prepend packages for a specific runtime or ecosystem with the name of that tool. For example, the pip conversion prepends all generated packages with `python-` to avoid conflicts with native libraries and show that they are specifically part of the python runtime/ecosystem.

We highly recommend adding metadata to each generated package spec so that it can be identified as auto-generated ([more about metadata]({{< ref "./create/spec" >}})). For example, the pip conversion process adds the following:

```yaml
meta:
  labels:
    spk:generated-by: spk-convert-pip
```
