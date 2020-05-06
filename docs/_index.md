---
title: spm
summary: Package Manager for SPFS
---

# SPM

The 'S' Package Manager.

Design Goals:

- convenience
- clarity
- speed
- automation

### Usage

```bash
# enter a blank spfs environment
spfs shell

# install python
spm install python/2.7

which python
# /spfs/bin/python

# get the latest version of vnp3
spm install vnp3
```

## Features

### Declarative Packages

```yaml
pkg: usd/20.02.4
depends:
- pkg: cmake/3.13
- pkg: open-exr/2.4
- pkg: alembic/1.7
- pkg: open-color-io/1.1
```

- TODO: differentiate build/run/private dependencies
  - possible an arbitrary purpose label (build, run)?

### Build Variants

```yaml
pkg: usd/20.02.4
depends:
- pkg: cmake/3.13
- pkg: open-exr/2.4
  variants: [default] # <- implied
- pkg: maya/2019
  variants: [maya-2019]
- pkg: maya/2018
  variants: [maya-2018]
```

```sh
spm build maya-2018
spm build -a
spm build default
```

### Include Multiple Versions of A Package

```yaml
pkg: usd/20.02.4
depends:
- pkg: cmake/3.13
  inclusion: SingleVerison # <- implied
- pkg: open-exr/2.4
  inclusion: LayerByVersion
- pkg: open-exr/2.5
  inclusion: LayerByVersion
# usually you would not include both versions in a single package
# definition, rather set the inclusion rule for a package at a higher
# level when a conflict arises
# this example shows that we can tell the system to include multiple
# versions of a package by layering them, ordered by version
# TODO: can this be related to the version compatibilty of a package?

# TODO: define behaviour for what to do with conflicting files
#       between two packages
- pkg: open-exr/2.6
  inclusion: LayerByVersion
  coverage: Error # do not allow existing files to be squashed by this one
```

```sh
# TODO: some kind of command to show/explain variants based on yaml
spm build-config
# python37:
#   SPM_OPT_python-abi = cp27
#   SPM_PKG_python = 2.7
# python27:
#   SPM_OPT_python-abi = cp27
#   SPM_PKG_python = 2.7
```

### Rebuild On Demand

- option declaration and propagation to build tree
- source definition and fetching
- automatic source detection for in-tree declaration
- can change build parameters and build new binaries as necessary

### RPath / Prefix Relocation

- detect rpaths in packaged binaries
- label packages that link outside of spfs?
- compatibility tool to identify prossible issues in new system (centos8)

### User Packages, Staging Environments, Package Promotion

- builds get captured using users email address
- can be pushed and shared this way
- additional channels/repos exist for package promotion
- deployment is promotion to some kind of staging channel, release channel

### Build Options and Arbitrary Label Compatibility

```yaml
pkg: maya-2019
opts:
- opt: os # <- uses current 'latest' aka current os (implied)
- opt: arch/any # <- overrides default / current
- opt: python-abi/cp27
  variants: [python37]
- opt: python-abi/cp37
  variants: [python27]
- opt: gcc/4.8  # SPM_OPT_gcc=4.8
- opt: gcc/6.3  # SPM_OPT_gcc=6.3
  inherit: Always # do users of this package inherit the gcc option?
                  # this ideas is useful for defining options about / in a package
                  # itself (aka gcc defines option and forces inheritence downstream)
# TODO: refine this, what differentiates package/option and
#       what is the value of the option as separate. They are
#       meant to represent build parameters, so what does that mean?
```

- arch, os, platform handled by the system automatically
  - compiler is related to this, does it get included?
  - can packages define options which cannot be changed?
    - (if the source is not available, then you cannot rebuild)

### Local Dependency Usage / Detection

- to link spdev components
- able to build library and then build plugin against it w/out issue and w/out pain
- ideally this is a fairly tight development loop
