---
title: Creating Packages
summary: Write package spec files for creating packages.
weight: 20
---

The package specification (spec) file is a yaml or json file which follows the structure detailed below.

### Name and Version

The only required field in a package spec file is the name and version number of the package. This is specified in the top-level `pkg` field. This field specifies the name and version number of the package being defined.

```yaml
pkg: my-package/1.0.0
```

### Compatibility

The optional `compat` field of a package specifies the compatibility between versions of this package. The compat field takes a version number, with each digit replaced by one or more characters denoting compatibility (`a` for api compatibility, `b` for binary compatbility and `x` for no compatibility). Multiple characters can be put together if necessary: `x.ab`.

If not specified, the default value for this field is: `x.a.b`.

```yaml
pkg: my-package/1.0.0
compat: x.a.b
# where major verions are not compatible
# minor versions are API-compatbile
# patch versions are ABI compatible.
```

The compat field of the new version is checked before install/update. Because of this, the compat field is more af a contract with past versions rather than future ones. Although it's recommended that your version compatibility remain constant for all versions of a package, this is not strictly required.


### Build Configutration

The build section of the package spec tells spk how to properly compile and cature your software as a package.

### Options

Build options are considered inputs to the build process. There are two types of options that can be specified: package options are build dependencies and var options are arbitrary configuration values for the build.


```yaml
build:
  options:
    - var: debug
      default: off
    - pkg: cmake/3
```

All options that are declared in your package should be used in the build script, otherwise they are not relevant build options and your package may need rebuilding unnecessarily.

When writing your build script, the value of each option is made available in an environment variable with the name `SPK_OPT_{name}`.

#### Script

```yaml
build:
  options:
    ...
  script:
    - mkdir -p build; cd build
    - CONFIG=Release
    - if [ "${SPK_OPT_debug}" == "on" ] CONFIG=Debug
    - cmake ..
      -DCMAKE_BUILD_TYPE=$CONFIG
      -DCMAKE_PREFIX_PATH=/spfs
      -DCMAKE_INSTALL_PREFIX=/spfs
    - cmake --build . --target install
```

The build script is bash code which builds your package. The script is responsible for installing your software into the `/spfs` directory.

spk assumes that your installed files will be layed out similarly to the unix statndard filesystem hierarchy. Most build systems can be configured with a **prefix**-type argument like the cmake example above which will handle this for you. If you are create python code, spk works just like an python virtual environment, and your code can be pip-installed using the /spfs/bin/pip that is included in the spk python packages or by manually copying to the appropriate `/spfs/lib/python<version>/site-packages` folder.

{{% notice tip %}}
If your build script is getting long or feels obstructive in your spec file, you can also create a build.sh script in your source tree which will be run if no build script is specified.
{{% /notice %}}

#### Variants

```yaml
build:
  options:
    ...
  variants:
    - {gcc: 6.3, debug: off}
    - {gcc: 6.3, debug: on}
    - {gcc: 4.8, debug: off}
    - {gcc: 4.8, debug: on}
  script:
    ...
```

The variants section of the build config defines the default set of variants that you want to build when running `spk build` and `spk make-binary`. Additional variants can be built later on, but this is a good way to streamline the default build process and define the set of variants that you want to support for every change.

### Install Configuration

The install configuration specifies the environment that your package needs when it is installed or included in an spk environment.

#### Requirements

Packages often require other packages to be present at run-time. These requirements should be listed in the `install.requirements` section of the spec file, and follow the same semantics as build options above.

```yaml
install:
  requirements:
  - pkg: python/2.7
```
