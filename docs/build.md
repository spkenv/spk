---
title: Package Build Process
summary: Detailed breakdown of how packages are built, and advanced build techniques
weight: 30
---


## General Build Process

The `spk build` command runs in two distinct phases, each with a small number of steps:

1. make the source package
  1. load the package spec
  1. setup a source directory under `/spfs/spk/pkg/<name>/<version>/src/`
  1. collect the source files defined in the [package yaml file](../spec#Sources)
  1. validate the collected source files
  1. store the source package and it's spec in the local repository
1. make the binary package
  1. load the package spec
  1. resolve the build environment / build dependencies
  1. execute the build script (in the source directory, see above)
  1. purge any changes made under the source directory
  1. validate the installed package files
  1. store the package and it's spec in the local repository

## Source Package Generation

By default, spk generates a source package for every package that you build. Source packages allow other developers to build new variants of your package on-demand when a build does not already exist for the environment that they are trying to resolve.

Source packages consist of a single directory under `/spfs` based on the package name and version. For example, `my-package/1.0.0/src` will have the directory `/spfs/spk/pkg/my-package/1.0.0/src/`. This directory will contain all the source files for that package version.

Source files are gathered based on the [sources](../spec#sources) section of the package specification.

## Binary Package Generation

There are two ways that a binary package can be built, using an existing source package, or an external set of source files.

{{% notice tip %}}
When building a package on the command line, it will build all variants of the package by default. You can use the `-o` flag to further select which variants should be build (or specify an entirely new variant).
{{% /notice %}}

### From a Source Package

Usually, the `spk build` command will create a source package first and then build it so building from a source package is the most common scenario.

When building from a source package, the build environment will be resolved with all necessary build dependencies, plus the source package. A build script will be generated based on the packages build configuration, and then executed from the source packages directory - [see 'Source Package Generation', above](#Source Package Generation).

Assuming that the build script completes successfully, spk will reset the sources area in spfs, removing all build artifacts that were not installed. For this reason, your build script can make any changes to the source files that are needed, but no files in this area will become part of the published package.

The most common example of this process is when using cmake. The build script usally creates a `build` folder in the source directory to compile in, but then installs the final binaries into `/spfs/lib`, `/spfs/bin` etc. For a build of `my-package`, for example, when the build script is finished I will have made changes in three locations:

```
/spfs/spk/pkg/my-package/1.0.0/src/build <- directory added for cmake config/build
/spfs/bin/my-package <- binary installed by cmake
/spfs/lib/libmy-package.so <- shared object installed by cmake
```

`spk` will reset the source folder, removing the `build` directory entirely. Any other remaining changes to `/spfs` are then validated and captured as the binary package. (`bin/my-package`, and `lib/my-package.so`, in this case).

### From External Sources

Binary packages can be created without the use of source packages by running the `spk make-binary` command and adding the `--here` flag. This flag tells spk that the build script should be run in the current directory, which is often helpful for quickly iterating on a local set of source files.

In this scenario, spk will not do anything to the source folder after build, allowing build artifacts and chaches to be maintained and reused between builds.
