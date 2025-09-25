---
title: Build Script and Configuration
summary: Defines how to compile and install your software.
weight: 30
---

The build section of the package spec tells spk how to properly compile and install your software for use as a package.

### Options

Build options are considered inputs to the build process. There are two types of options that can be specified: **package options** are build dependencies and **var options** are arbitrary configuration values for the build.

```yaml
build:
  options:
    - var: debug/off
      choices: [on, off]
    - pkg: cmake/3.16
```

All options that are declared in your package should be used in the build script, otherwise they are not relevant build options and your package may need rebuilding unnecessarily.

When writing your build script, the value of each option is made available in an environment variable with the name `SPK_OPT_{name}`. Package options are also resolved into the build environment and can be accessed more concretely with the variables `SPK_PKG_{name}`, `SPK_PKG_{name}_VERSION`, `SPK_PKG_{name}_BUILD`, `SPK_PKG_{name}_VERSION_MAJOR`, `SPK_PKG_{name}_VERSION_MINOR`, `SPK_PKG_{name}_VERSION_PATCH`

> [!TIP]
> Best practice for defining boolean options is to follow the cmake convention of having two choices: `on` and `off`

> [!TIP]
> Best practice for package requirements is to specify a minimum version number only, and leverage the compatibility specification defined by the package itself rather than enforcing something else (eg use `default: 3.16` instead of `default: ^3.16`)

#### Common Build Options

There are some build options that are either provided by the system or are used enough to create a common convention.

| Option Name          | Value(s)                                       | Example                |
| -------------------- | ---------------------------------------------- | ---------------------- |
| arch                 | The build architecture                         | x86_64, i386, ...      |
| os                   | The operating system                           | linux, windows, darwin |
| distro               | The linux distribution, if applicable          | almalinux, ubuntu, ... |
| almalinux,ubuntu,etc | The linux distro version number, if applicable | 9.5, 9.6, ...          |
| debug                | Denotes a build with debug information         | on, off                |

#### Build Variable Description

For build variables, a description of up to 256 characters can be provided.

```yaml
build:
  options:
    - var: color/blue
      choices: [red, blue, green]
      inheritance: Strong
      description: |
        Control what color the lights will be when lit.
```

When a downstream package depends on this package the description will also get propagated into the build.

```yaml
pkg: user-of-lights/1.0.0/BUILDGST
...
install:
  requirements:
    - pkg: lights/Binary:1.0.0
    - var: lights.color/green
      description: |
        Control what color the lights will be when lit.
```

If a longer description is required, the [validation](#validation) rule `LongVarDescription` can be used to reconfigure the validation process to allow for longer descriptions:

```yaml
build:
  validation:
    rules:
      - allow: LongVarDescription
```

Furthermore, strong inheritance variables will require a description. To also reconfigure this validation process, the [validation](#validation) rule `StrongInheritanceVarDescription` can be used to disable this validation.

```yaml
build:
  validation:
    rules:
      - deny: StrongInheritanceVarDescription
```

### Script

```yaml
build:
  options: ...
  script:
    - mkdir -p build; cd build
    - CONFIG=Release
    - if [ "${SPK_OPT_debug}" == "on" ]; then CONFIG=Debug; fi
    - cmake ..
      -DCMAKE_BUILD_TYPE=$CONFIG
      -DCMAKE_PREFIX_PATH=/spfs
      -DCMAKE_INSTALL_PREFIX=/spfs
    - cmake --build . --target install
```

The build script is bash code which builds your package. The script is responsible for installing your software into the `/spfs` directory.

spk assumes that your installed files will be laid out similarly to the unix standard filesystem hierarchy. Most build systems can be configured with a **prefix**-type argument like the cmake example above which will handle this for you. If you are create python code, spk works just like an python virtual environment, and your code can be pip-installed using the /spfs/bin/pip that is included in the spk python packages or by manually copying to the appropriate `/spfs/lib/python<version>/site-packages` folder.

> [!TIP]
> If your build script is getting long or feels obstructive in your spec file, you can also create a `build.sh` script in your source tree next to the spec file. This file will be run if no build script is specified in yaml.

### Variants

```yaml
build:
  options: ...
  variants:
    - { gcc: 6.3, debug: off }
    - { gcc: 6.3, debug: on }
    - { gcc: 4.8, debug: off }
    - { gcc: 4.8, debug: on }
  script: ...
```

The variants section of the build config defines the default set of variants that you want to build when running `spk build` and `spk make-binary`. Additional variants can be built later on, but this is a good way to streamline the default build process and define the set of variants that you want to support for every change.

By default, the command line will build all variants defined in your spec file. Supplying any options on the command line will instead build only a single variant using specified options.

Variants can introduce new package options, making a build dependency only
required when building that variant.

When specifying a package option in a variant, it can name one or more
components to only add a dependency on those components. The resulting set of
components is the union of the ones specified in the variant and any existing
components from the entry in `build.options` (if any).

```yaml
build:
  options:
    - pkg: foo:data/1.0
  variants:
    # This variant will depend on `foo:{data,docs}/1.0`.
    - { "foo:docs": "1.0" }
    # This variant will depend on `foo:{data,docs,examples}/2.0`.
    - { "foo:{docs,examples}": "2.0" }
```

> [!TIP]
> Build requirements can also be updated in the command line: `spk install --save @build build-dependency/1.0`

### Validation

The spk build system performs a number of validations against the package created during a build. These validators can be overridden and further refined using the `validation` portion of the build spec. See [validation rules]({{< ref "../../ref/api/v0/package" >}}#validationspec)
