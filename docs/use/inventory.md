---
title: Inventorying a Repository
summary: Using the spk inventory command to get information on package depth, dependencies, and what uses them
weight: 110
---


## spk inventory

The `spk inventory` command can trace which packages use a particular
package in a repository, and a package's full dependencies. It can
also calculate the depths of packages from the chains of dependencies
in a repository. This can be used to work out what order to recompile
packages in (such as for a new OS), and what other packages are needed
before a particular package can be recompiled.


`spk inventory ...` command can be run on one package, in which case
it will only show the dependencies of and packages that use the given
package, or on the entire repository of packages to do a full depth
analysis.

### On one package

When focusing on a package (`spk inventory <package>`), the output has three parts:

1. The packages at each depth that are in the chain of dependencies of your package
2. A list of all transitive dependencies of your package
3. The list of all packages that in turn depend on your package

The "Depth" is a measure of how deep the dependencies of a package go:
- Depth 0 are packages that have no other dependencies in spk
- Depth 1 are packages whose dependencies all have depth 0
- Depth n are packages whose dependencies contain at least one package with depth n-1, but no dependencies with depth >= n

The depths correspond to the recompilation order, e.g. if you had to
recompile everything for a new OS, with lower number depths would need
to be recompiled first.

For example, focusing on the python package (details elided for space):

```
> spk inventory python
Gathered package data from repos in:  0.475759666 secs
Focusing on: python

DEPTH 0
------------------------------
gcc (clients=552, xdeps=0) deps = []


DEPTH 1
------------------------------
bzip2 (clients=2, xdeps=0) deps = [gcc]

All transitive dependencies of python:
--------------------------------------
  - bzip2  (depth: 1)
  - gcc    (depth: 0)

All packages that use python:
-----------------------------
  - python-accelerate                                    (dir) (depth: 9)
  - python-utilities                                     (dir) (depth: 4)
  - python-test                                          (dir) (depth: 5)
...
  - usd                                                  (dir) (depth: 11)
  - usd-something                                              (depth: 15)
```

There is an option for changing the output format to json or yaml (`--format ...`). There are options for getting lists of the package's dependencies (`--deps`) or client/used by packages (`--used-by`):

For example, getting all the package this package depends on:
```
> spk inventory openimageio --deps
Gathered package data from repos in   : 0.467509538 secs
Focusing on: openimageio

Packages that 'openimageio' depends on:
boost
boost-python
bzip2
cmake
...
```

For example, getting all the other packages that use this package:
```
> spk inventory openimageio --used-by
Gathered package data from repos in   : 0.471124277 secs
Focusing on: openimageio

Packages that use 'openimageio':
...
bifrost-usd
maya-usd
opencolorio-apps
osl
some-widgets
usd
...
```

### On the whole repository

When run without a package (`spk inventory`), it operates on the
entire repository and outputs all the packages at each depth, from all
the chains of dependencies in the repository.

For example (details elided for space):
```
> spk inventory
Gathered package data from repos in   : 0.466984379 secs


DEPTH 0
------------------------------
cuda (clients=21, xdeps=0) deps = []
hugo (clients=0, xdeps=0) deps = []
...

DEPTH 12
------------------------------
check (clients=0, xdeps=55) deps = [..., gcc, python, python-jinja2, python-pyside2] 
bifrost-usd (clients=0, xdeps=75) deps = [aw-bifrost, cmake, gcc, ..., python, usd]
some-widgets (clients=5, xdeps=82) deps = [..., imath, opencolorio, openimageio, python, ...]
...

DEPTH 23
------------------------------
maya-something (clients=0, xdeps=406) deps = [..., imath, maya-plugins, python, ...]
...
```
