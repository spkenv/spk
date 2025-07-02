---
title: Creating Packages
summary: Write package spec files for creating packages.
weight: 20
---

The package specification (spec) file is a yaml or json file which follows the structure detailed below. See the [Package Spec Schema]({{< ref "../ref/api/v0/package" >}}) for a more complete set of available fields.

### Name and Version

The only required field in a package spec file is the name and version number of the package. This is specified in the top-level `pkg` field. This field specifies the name and version number of the package being defined.

```yaml
pkg: my-package/1.0.0
```

{{% notice note %}}
Package names can only be composed of lowercase ascii letters, digits and dashes (`-`). This is done to try and make sure that packages are easier to find and predict, rather than having a whole bunch of different ways to name them (eg: myPackage, MyPackage, My_Package, my_package, my-package, etc...). This restricted character set also provides the greatest freedom for us extend the naming specification in the future, if needed.
{{% /notice %}}

### Compatibility

The optional `compat` field of a package specifies the compatibility between versions of this package. The compat field takes a version number, with each digit replaced by one or more characters denoting compatibility (`a` for api compatibility, `b` for binary compatibility and `x` for no compatibility). Multiple characters can be put together if necessary: `x.ab`.

If not specified, the default value for this field is: `x.a.b`. This means that at build time and on the command line, when API compatibility is needed, any minor version of this package can be considered compatible (eg `my-package/1.0.0` could resolve any `my-package/1.*`). When resolving dependencies however, when binary compatibility is needed, only the patch version is considered (eg `my-package/1.0.0` could resolve any `my-package/1.0.*`).

Pre-releases and post-releases of the same version are treated as compatible, however this can be controlled by adding an extra compatibility clause to the `compat` field. For example, `x.x.x-x+x` would mark a build as completely incompatible with any other build, including other pre- or post-releases of the same version.

```yaml
pkg: my-package/1.0.0
compat: x.a.b
# where major versions are not compatible
# minor versions are API-compatible
# patch versions are binary compatible
```

The compat field of the new version is checked before install/update. Because of this, the compat field is more af a contract with past versions rather than future ones. Although it's recommended that your version compatibility remain constant for all versions of a package, this is not strictly required.

### Sources

The `sources` section of the package spec tells spk where to collect and how to arrange the source files required to build the package. Currently, it defaults to collecting the entire directory where the spec file is loaded from, but can be overridden with a number of different sources.

#### Local Source

Local directories and files are simply copied into the source area. Paths here can be absolute, or relative to the location of the spec file. Git repositories (`.git`) and other source control files are automatically excluded, using the rsync `--cvs-exclude` flag. Furthermore, if a `.gitignore` file is found in the identified directory, then it will be used to further filter the files being copied.

```yaml
sources:
  # copy the src directory next to this spec file
  - path: ./src
  # copy a single file from the config directory
  # into the root of the source area
  - path: ./config/my_config.json
```

#### Git Source

Git sources are cloned into the source area, and can take an optional ref (tag, branch name, commit) to be checked out.

```yaml
sources:
  - git: https://github.com/qt/qt5
    ref: v5.12.9
```

#### Tar Source

Tar sources can reference both local tar files and remote ones, which will be downloaded first to a temporary location. The tar file is extracted automatically into the source area for use during the build.

```yaml
sources:
  - tar: https://github.com/qt/qt5/archive/v5.12.9.tar.gz
```

#### Script Source

Script sources allow you to write arbitrary bash script that will collect and arrange sources in the source package. The script is executed with the current working directory as the source package to be built. This means that the script must collect sources into the current working directory.

Any previously listed sources will already exist in the scripts current directory, and so the script source can also be used to arrange and adjust source files fetched through other means.

```yaml
sources:
  - script:
      - touch file.yaml
      - svn checkout http://myrepo my_repo_svn
```

#### Multiple Sources

You can include sources from multiple locations, but will need to specify a subdirectory for each source in order to make sure that they are each downloaded/fetched into their own location in the source package. Some sources can be intermixed into the same location (such as local sources) but others require their own location (such as git sources).

```yaml
sources:
  # clones this git repo into the 'someproject' subdirectory
  - git: https://github.com/someuser/someproject
    ref: main
    subdir: someproject
    # copies the contents of the spec file's location into the 'src' subdirectory
  - path: ./
    subdir: src
```

### Build Configuration

The build section of the package spec tells spk how to properly compile and capture your software as a package.

#### Options

Build options are considered inputs to the build process. There are two types of options that can be specified: package options are build dependencies and var options are arbitrary configuration values for the build.

```yaml
build:
  options:
    - var: debug/off
      choices: [on, off]
    - pkg: cmake/3.16
```

All options that are declared in your package should be used in the build script, otherwise they are not relevant build options and your package may need rebuilding unnecessarily.

When writing your build script, the value of each option is made available in an environment variable with the name `SPK_OPT_{name}`. Package options are also resolved into the build environment and can be accessed more concretely with the variables `SPK_PKG_{name}`, `SPK_PKG_{name}_VERSION`, `SPK_PKG_{name}_BUILD`, `SPK_PKG_{name}_VERSION_MAJOR`, `SPK_PKG_{name}_VERSION_MINOR`, `SPK_PKG_{name}_VERSION_PATCH`

{{% notice tip %}}
Best practice for defining boolean options is to follow the cmake convention of having two choices: `on` and `off`
{{% /notice %}}

{{% notice tip %}}
Best practice for package requirements is to specify a minimum version number only, and leverage the compatibility specification defined by the package itself rather than enforcing something else (eg use `default: 3.16` instead of `default: ^3.16`)
{{% /notice %}}

##### Common Build Options

There are some build options that are either provided by the system or are used enough to create a common convention.

| Option Name | Value(s)                                       | Example                |
| ----------- | ---------------------------------------------- | ---------------------- |
| arch        | The build architecture                         | x86_64, i386, ...      |
| os          | The operating system                           | linux, windows, darwin |
| distro      | The linux distribution, if applicable          | centos, ubuntu, ...    |
| centos      | The centos major version number, if applicable | 7, 8, ...              |
| debug       | Denotes a build with debug information         | on, off                |

##### Build Variable Description

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

#### Script

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

{{% notice tip %}}
If your build script is getting long or feels obstructive in your spec file, you can also create a build.sh script in your source tree which will be run if no build script is specified.
{{% /notice %}}

#### Variants

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

{{% notice tip %}}
Build requirements can also be updated in the command line: `spk install --save @build build-dependency/1.0`
{{% /notice %}}

#### Validation

The spk build system performs a number of validations against the package created during a build. These validators can be overridden and further refined using the `validation` portion of the build spec. See [validation rules]({{< ref "../ref/api/v0/package" >}}#validationspec)

### Install Configuration

The install configuration specifies the environment that your package needs when it is installed or included in an spk environment.

{{% notice info %}}
Packages that only provide opinions/constraints on an environment, but no actual dependencies or content, are considered 'platform' packages. SPK provides a native spec for this use case, see {{< ref "./platforms" >}}.
{{% /notice %}}

#### Environment Variables

Packages can append, prepend and set environment variables at runtime if needed. Furthermore, you are able to add comments and set the priority of the generated activation script. It's strongly encouraged to only modify variables that your package can reasonably take ownership for. For example, the `python` package should be the only one setting `PYTHON*` variables that affect the runtime of python. This is not an enforced rule, but if you find yourself setting `PYTHONPATH`, for example, then it might mean that you are installing to a non-standard location within spfs and breaking away from the intended consistency of spfs.

```yaml
install:
  environment:
    - priority: 99
    - comment: START
    - set: MYPKG_VAR
      value: hello, world
    - append: PATH
      value: /spfs/opt/mypkg/bin
    - comment: END
```

The above example will generate the activation scripts `99_spk_{package_name}.csh` and `99_spk_{package_name}.sh`

#### Requirements

Packages often require other packages to be present at run-time. These requirements should be listed in the `install.requirements` section of the spec file, and follow the same semantics as build options above.

```yaml
install:
  requirements:
    - pkg: python/2.7
```

You can also reference packages from the build environment in your installation requirements. This is the recommended way to connect the build environment with the run-time environment, so that the install requirements can change with each variant that is generated. For example, creating a python package for both python 2 and python 3 can use this feature to make sure that the same version of python is used at run-time.

```yaml
build:
  options:
    - pkg: python
  variants:
    - { python: 2 }
    - { python: 3 }
install:
  requirements:
    - pkg: python
      fromBuildEnv: x.x
```

In this example, we might get two build environments, one with `python/2.7.5` and one with `python/3.7.3`. These version numbers will be used at build time to pin an install requirement of `{pkg: python/2.7}` and `{pkg: python/3.7}`, respectively.

{{% notice tip %}}
Install requirements can also be updated in the command line: `spk install --save @install build-dependency/1.0`
{{% /notice %}}

##### Build Variable Requirements

You can also place constraints on specific build options at install time. This is most useful for identifying stricter compatibility requirements for your package dependencies. For example, native python modules generally require that the version of python being used have the same binary interface as the one which the module was built against. In such an example, the `abi` build option from the python package can be constrained as a requirement:

```yaml
install:
  requirements:
    - pkg: python
      fromBuildEnv: x.x
    # require that the same python abi be used at install time
    - var: python.abi
      fromBuildEnv: true
```

{{% notice tip %}}
Variable requirements can also be specified statically in the form `name/value` (eg `- var: python.abi/cp37`)
{{% /notice %}}

##### Optional Requirements

Sometimes, you're package does not directly require another package, but would like to impose a constraint _if_ that package is required by something else. An example of this might be a cpp library with python bindings. The cpp library can be used without python, but if python exists in the environment, then we want to make sure it's of a compatible version.

The `include` field allows you to specify how a requirement should be applied to the environment.

```yaml
install:
  requirements:
    - pkg: python/2.7
      # if python is already in the environment/resolve then we
      # we require it to be compatible with 2.7
      # but no python at all is also okay
      include: IfAlreadyPresent
```

#### Components

Every package in spk is divided into multiple components. The `build` and `run` components are always present, and are intended to represent the set of files needed when building against the package vs simply running against the software within. By default, the `build` and `run` components will be the same, but you can help ensure that downstream consumers only get what they need by refining what these components include.

```yaml
install:
  components:
  - name: run
    # only the compiled libraries are needed at runtime
    files: [lib/mylib*.so]
  - name: build
    # but everything else (debug symbols or static libraries, for example)
    # should be pulled in when building against this package
    files: ['*']
```

Packages can also define any number of supplementary components which contain some subset of the files created by the build process. These might be used to separate a software library from executables, or static from dynamic libraries. Ultimately, the goal is to define useful sets of files so that downstream consumers only need to pull in what they actually need from your package.

Additionally, components can also declare simple dependencies on one another, which is referred to as one component _using_ another.

```yaml
install:
  components:
  - name: lib
    # files follow the same semantics as a gitignore/gitinclude file
    files: [lib/mylib*.so]
  - name: bin
    uses: lib
    files: [bin/]
  - name: run
    uses: [lib, bin]
  - name: build
    uses: [run]
```

Finally, you can extend and augment both the requirements and embedded packages for each component. These are added on top of any requirements or embedded packages defined at the install level.

```yaml
install:
  requirements:
    - pkg: python
  components:
  - name: bin
    requirements:
      # narrow the package requirement for python to
      # exactly python 3.7.3 for this component
      - pkg: python/=3.7.3
      # add a new requirement for this component
      - pkg: python-requests
```

#### Embedded Packages

Some software, like Maya or other DCC applications, come bundled with their own specific version of many libraries. SPK can represent this bundled software natively, so that environments can be properly resolved using it. For example, Maya bundles its own version of `qt`, and no other version of qt should be resolved into the environment. By defining `qt` as an embedded package, users who request environments with both `maya` and `qt`, will have qt resolved to the one bundled in the `maya` package, if compatible. If maya embeds `qt/5.12` but the user requests `qt/4.8` then the resolve will fail as expected since this environment is unsafe.

```yaml
pkg: maya/2019.2.0
install:
  embedded:
    - pkg: qt/5.12.6
```

Embedded packages can also define build options where compatibility with some existing package of the same name is desired, for example:

```yaml
pkg: maya/2019.2.0
install:
  embedded:
    - pkg: python/2.7.11
      build:
        options:
          - { var: abi, static: cp27m }
```

### Testing

Tests can also be defined in the package spec file. SPK currently supports three types of tests that validate different aspects of the package. Tests are defined by a bash script and _stage_.

```yaml
pkg: my-package/1.0.0

# the tests section can define any number of
# tests to validate the package
tests:
  - stage: build
    script: python -m "unittest"
```

#### Stages

The **stage** of each test identifies when and where the test should be run. There are three stages that can currently be tested:

| stage   | description                                                                                             |
| ------- | ------------------------------------------------------------------------------------------------------- |
| sources | runs against the created source package, to validate that source files are correctly laid out           |
| build   | runs in the package build environment, usually for unit testing                                         |
| install | runs in the installation environment against the compiled package, usually for integration-type testing |

#### Variant Selectors

Like builds, tests are executed by default against all package variants defined in the build section of the spec file. Each test can optionally define a list of selectors to reduce the set of variants that is is run against.

```yaml
build:
  variants:
    - { python: 3 }
    - { python: 2 }

tests:
  - stage: install
    selectors:
      - { python: 3 }
    script:
      - "test python 3..."

  - stage: install
    selectors:
      - { python: 2 }
    script:
      - "test python 2..."
```

The test is executed if the variant in question matches at least one of the selectors.

{{% notice info %}}
Selectors must match exactly the build option values from the build variants. For example: a `python: 2.7` selector will not match a `python: 2` build variant.
{{% /notice %}}

#### Requirements

You can specify additional requirements for any defined test. These requirements are merged with those of test environment so be sure that they do not conflict with what you are testing.

```yaml
build:
  options:
    - pkg: python/3

tests:
  - stage: install
    requirements:
      - pkg: pytest
    script:
      - pytest
```

### Spec File Templating

SPK package spec files also supports the `jinja2` templating language via the [tera library in Rust](https://keats.github.io/tera/docs/#templates), so long as the spec file remains valid yaml. This means that often, templating logic is best placed into yaml comments, with some examples below.

The templating is rendered when the yaml file is read from disk, and before it's processed any further (to start a build, run tests, etc.). This means that it cannot, for example, be involved in rendering different specs for different variants of the package (unless you define and orchestrate those variants through a separate build system).

The data that's made available to the template takes the form:

```yaml
spk:
  version: "0.23.0" # the version of spk being used
opt: {} # a map of all build options specified (either host options or at the command line)
env: {} # a map of the current environment variables from the caller
```

One common templating use case is to allow your package spec to be reused to build many different versions, for example:

```yaml
# {% set opt = opt | default_opts(version="2.3.4") %}
pkg: my-package/{{ opt.version }}
```

Which could then be invoked for different versions at the command line:

```sh
spk build my-package.spk.yaml                  # builds the default 2.3.4
spk build my-package.spk.yaml -o version=2.4.0 # builds 2.4.0
```

#### Template Extensions

In addition to the [default functions and filters](https://keats.github.io/tera/docs/#built-ins) within the tera library, spk provides a few additional ones to help package maintainers:

##### Filters

**default_opts**

The `default_opts` filter can be used to more easily declare default values for package options that can be overridden on the command line. The following two blocks are equivalent:

```jinja
{% set opt.foo = opt.foo | default(value="foo") %}
{% set opt.bar = opt.bar | default(value="bar") %}

{% set opt = opt | default_opts(foo="foo", bar="bar") %}
```

An additional benefit of the second block is that the names of options and their values will be validated using the spk library. Either approach is valid, depending on the use case and preferences.

**compare_version**

The `compare_version` allows for comparing spk versions using any of the [version comparison operators]({{< ref "./versioning" >}}). It takes one or two arguments, depending on the data that you have to give. In all cases, the arguments are concatenated together and parsed as a version range. For example, the following assignments to py_3 all end up checking the same statement.

```jinja
{% set python_version = "3.10" %}
{% set is_py3 = python_version | compare_version(op=">=3") %}
{% set is_py3 = python_version | compare_version(op=">=", rhs=3) %}
{% set three = 3 %}
{% set is_py3 = python_version | compare_version(op=">=", rhs=three) %}
```

**parse_version**

The `parse_version` filter breaks down an spk version string into its components, either returning an object or a single field from it, for example:

```jinja
{% assign v = "1.2.3.4-alpha.0+r.4" | parse_version %}
{{ v.base }}      # 1.2.3.4
{{ v.major }}     # 1
{{ v.minor }}     # 2
{{ v.patch }}     # 3
{{ v.parts[3] }}  # 4
{{ v.post.r }}    # 4
{{ v.pre.alpha }} # 0
{{ "1.2.3.4-alpha.0+r.4" | parse_version(field="minor") }} # 2
```

**replace_regex**

The `replace_regex` filter works like the built-in `replace` filter, except that it matches using a perl-style regular expression and allows group replacement in the output. These regular expressions do not support look-arounds or back-references. For example:

```jinja
{% set version = opt.version | default("2.3.4") %}
{% set major_minor = version | replace_regex(from="(\d+)\.(\d+).*", to="$1.$2") %}
{{ major_minor }} # 2.3
```

### Recursive Builds

By default, builds will fail if another version of the package being built ends up in the build environment, either as a direct or indirect dependency. There are packages, however, that bootstrap their own build process and require this (for example: compilers like gcc or package systems like pip). Furthermore, these recursive builds often perform an in-place upgrade, writing over some or all the previous versions files which is typically not allowed.

The [validation](#validation) rule `RecursiveBuild` can be used to reconfigure the validation process for these scenarios:

```yaml
build:
  validation:
    rules:
      - allow: RecursiveBuild
```
