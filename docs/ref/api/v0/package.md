---
title: package
summary: Detailed package specification information
aliases: ['/ref/spec']
---

This document details each data structure and field that does or can exist within a package spec file for spk.

## Package Spec

The root package spec defines which fields can and should exist at the top level of a spec file.

| Field      | Type                              | Description                                                                                                                                           |
| ---------- | --------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg        | _[Identifier](#identifier)_       | The name and version number of this package                                                                                                           |
| meta       | [Meta](#meta)                     | Extra package metadata such as description, license, etc                                                                                              |
| compat     | _[Compat](#compat)_               | The compatibility semantics of this packages versioning scheme                                                                                        |
| deprecated | _boolean_                         | True if this package has been deprecated, this is usually reserved for internal use only and should not generally be specified directly in spec files |
| sources    | _List[[SourceSpec](#sourcespec)]_ | Specifies where to get source files for building this package                                                                                         |
| build      | _[BuildSpec](#buildspec)_         | Specifies how the package is to be built                                                                                                              |
| tests      | _List[[TestSpec](#testspec)]_     | Specifies any number of tests to validate the package and software                                                                                    |
| install    | _[InstallSpec](#installspec)_     | Specifies how the package is to be installed                                                                                                          |

## Meta

| Value       | Type           | Description                                                            |
| ----------- | -------------- | ---------------------------------------------------------------------- |
| description | _str_          | (Optional) A concise, one sentence description of the package          |
| homepage    | _str_          | (Optional) URL where the package lives                                 |
| license     | _str_          | (Optional) Package license. If not specified, defaults to _Unlicensed_ |
| labels      | _Map[str,str]_ | (Optional) A storage for arbitrary key-value data                      |

## SourceSpec

A source spec can be one of [LocalSource](#localsource), [GitSource](#gitsource), or [TarSource](#tarsource).

### LocalSource

Defines a local directory to collect sources from. This process will also automatically detect a git repository and not transfer ignored files.

| Field   | Type        | Description                                                                                       |
| ------- | ----------- | ------------------------------------------------------------------------------------------------- |
| path    | _str_       | The relative or absolute path to a local directory                                                |
| exclude | _List[str]_ | A list of glob patterns for files and directories to exclude (defaults to `".git/", ".svn/")      |
| filter  | _List[str]_ | A list of filter rules for rsync (defaults to reading from the gitignore file: `":- .gitignore"`) |
| subdir  | _str_       | An alternative path to place these files in the source package                                    |

### GitSource

Clones a git repository as package source files.

| Field  | Type  | Description                                                    |
| ------ | ----- | -------------------------------------------------------------- |
| git    | _str_ | The url or local path to a git repository to be cloned         |
| ref    | _str_ | Optional branch, commit or tag name for the source repo        |
| subdir | _str_ | An alternative path to place these files in the source package |

### TarSource

Fetches and extracts a tar archive as package source files.

| Field  | Type  | Description                                                    |
| ------ | ----- | -------------------------------------------------------------- |
| tar    | _str_ | The url or local path to tar file                              |
| subdir | _str_ | An alternative path to place these files in the source package |

## BuildSpec

| Field          | Type                                | Description                                                                                                                                         |
| -------------- | ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| script         | _str_ or _List[str]_                | The bash script which builds and installs the package to /spfs                                                                                      |
| options        | _List[[BuildOption](#buildoption)]_ | The set of inputs for the package build process                                                                                                     |
| variants       | _List[[VariantSpec](#variantspec)]_ | The default variants of the package options to build                                                                                                |
| validation     | _[ValidationSpec](#validationspec)_ | Modifies the default package validation process                                                                                                     |
| auto_host_vars | _[AutoHostVars](#autohostvars)_     | The host compatibility setting for the package's builds. Depending on the value, it injects build options like distro, arch, os, and distro version |


### BuildOption

A build option can be one of [VariableOption](#variableoption), or [PackageOption](#packageoption).

#### VariableOption

Variable options represents some arbitrary configuration parameter to the build. When the value of this string changes, a new build of the package is required. Some common examples of these options are: `arch`, `os`, `debug`.

| Field       | Type        | Description                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ----------- | ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| var         | _str_       | The name of the option, with optional default value (eg `my_option` or `my_option/default_value`)                                                                                                                                                                                                                                                                                                                                 |
| choices     | _List[str]_ | An optional set of possible values for this variable                                                                                                                                                                                                                                                                                                                                                                              |
| inheritance | _str_       | Defines how this option is inherited by downstream packages. `Weak` is the default behaviour and does not influence downstream packages directly. `Strong` propagates this build option into every package that has this one in it's build environment while also adding an install requirement for this option. `StrongForBuildOnly` can be used to propagate this requirement as a build option but not an install requirement. |
| static      | _str_       | Defines an unchangeable value for this variable - this is usually reserved for use by the system and is set when a package build is published to save the value of the variable at build time                                                                                                                                                                                                                                     |

#### PackageOption

Package options define a package that is required at build time.

| Field            | Type                                    | Description                                                                                                                                                                                    |
| ---------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg              | _str_                                   | The name of the package that is required, with optional default value (eg just `package_name`, or `package_name/1.4`)                                                                          |
| prereleasePolicy | _[PreReleasePolicy](#prereleasepolicy)_ | Defines how pre-release versions should be handled when resolving this request                                                                                                                 |
| static           | _str_                                   | Defines an unchangeable value for this variable - this is usually reserved for use by the system and is set when a package build is published to save the version of the package at build time |

### VariantSpec

A VariantSpec is a key-value mapping that describes a desired combination of
options or additional packages to use when building a package. By defining
multiple variants, it is possible to build a package multiple ways with
different versions of dependencies.

Each entry in the VariantSpec can either:

- Set or override the value of an existing variable option:

  ```yaml
  build:
    options:
      - var: debug
    variants:
      - { debug: on }
      - { debug: off }
  ```

- Override the version of an existing package option:

  ```yaml
  build:
    options:
      - pkg: gcc/6.3
    variants:
      - { gcc: "6.3" }
      - { gcc: "9.3" }
  ```

- Add an additional component dependency of an existing package option:

  ```yaml
  build:
    options:
      - pkg: foo/1.0
    variants:
      - { "foo:docs": "1.0" }
      - { "foo:{docs,examples}": "2.0" }
  ```

- Introduce a new package option:

  ```yaml
  build:
    options:
      - pkg: foo/1.0
    variants:
      - { "bar": "1.0" }
      - { "bar:{extra1,extra2}": "2.0" }
  ```

### ValidationSpec

The ValidationSpec modifies the default validation process for packages, primarily providing the ability to disable validators which may be incorrectly failing a package build.

| Field    | Type                                      | Description                                                                                               |
| -------- | ----------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| rules    | _List[[ValidationRule](#validationrule)]_ | The set of rules applied to this package (not allowed when `disabled` is given)                           |
| disabled | _List[str]_                               | Default validators to disable, see [Validators](#validators-deprecated) (deprecated, use `rules` instead) |

#### ValidationRule

| Field   | Type  | Description                                          |
| ------- | ----- | ---------------------------------------------------- |
| allow   | _str_ | If matched, the package is still considered valid    |
| deny    | _str_ | When matched, the package will be deemed invalid     |
| require | _str_ | When not matched, the package will be deemed invalid |

Each validation rule may have additional properties which allow it to be further configured as noted below. You can specify the same rule more than once with different properties depending on the use case, and the last matched instance for any particular validation will be taken as the final result, unless an earlier rule was more specific. For example:

```yaml
build:
  validation:
    rules:
      # override the default rule by allowing the build
      # to modify files from other packages
      - allow: AlterExistingFiles
      # Refine the above rule by not allowing modification
      # of files from the python or gcc packages. Because this
      # rule is more specific than the last (names individual packages)
      # it will override the above rule no matter which order they
      # appear in this list
      - deny: AlterExistingFiles
        packages: [python, gcc]
```

##### Available Validation Rules

| Name (default)                 | Property | Type          | Description                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------ | -------- | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| EmptyPackage (Deny)            |          |               | Matched when no files are installed to spfs during the build                                                                                                                                                                                                                                                                                                                             |
| AlterExistingFiles (Deny)      |          |               | Matched when a package modifies files from other packages when building                                                                                                                                                                                                                                                                                                                  |
|                                | packages | _List[_str_]_ | Only match when the modified files belong to one of these named packages                                                                                                                                                                                                                                                                                                                 |
|                                | action   | _str_         | Only match this type of change, one of `Change`, `Remove`, or `Touch`                                                                                                                                                                                                                                                                                                                    |
| CollectExistingFiles (Deny)    |          |               | Matched when a package collects files from other packages in the build environment                                                                                                                                                                                                                                                                                                       |
|                                | packages | _List[_str_]_ | Only match when the modified files belong to one of these named packages. The special `Self` value can be used to refer to the current package's name.                                                                                                                                                                                                                                   |
| InheritRequirements (Required) |          |               | Matched when a package in the build environment has an inherited requirement that is not present in the package generated by this build.                                                                                                                                                                                                                                                 |
|                                | packages | _List[_str_]_ | Only match when the inherited requirement comes from one of these named packages.                                                                                                                                                                                                                                                                                                        |
| RecursiveBuild (Deny)          |          |               | Matched when the build environment contains another version of the package being built. This rule implicitly enables rules to allow modifying and collecting files from the previous version of this package. Additional rules can be added to reverse these implicit ones                                                                                                               |
| SpdxLicense (Allow)            |          |               | Matched when the package being built has a valid spdx license identifier in the metadata (meta.license). Use `Require` to ensure that a license is provided and valid. `Allow` ensures that a provided value is valid but also allows no license. `Deny` can be used to ensure no license is specified. Remove the validation altogether if a custom license is needed (not recommended) |

For example:

```yaml
build:
  validation:
    # Allow recursive builds, aka building a new version of this package
    # using a previous version.
    - allow: RecursiveBuild
    # Reverse the implicit rule from above that would allow including files
    # from the previous version of this package
    - deny: CollectExistingFiles
      packages: [Self]
```

#### Validators (deprecated)

| Name                      | Default | Description                                                                                                               |
| ------------------------- | ------- | ------------------------------------------------------------------------------------------------------------------------- |
| MustInstallSomething      | Enabled | Packages must install at least one file or folder during build                                                            |
| MustNotAlterExistingFiles | Enabled | Packages must not modify the content or metadata of any file that is provided by another package in the build environment |

### AutoHostVars

The AutoHostVars value sets which host- and os-related options are
automatically added to each build. The values add zero, or more, host
options to each build, as described in the table:

| Value                | Adds these host var options                      | Examples of added host var options             |
| -------------------- | ------------------------------------------------ | ---------------------------------------------- |
| **Distro** (default) | "distro", "arch", "os", and the "\<distroname\>" | distro=centos, arch=x86_64, os=linux, centos=7 |
| **Arch**             | "arch", "os"                                     | arch=x86_64, os=linux                          |
| **Os**               | "os"                                             | os=linux                                       |
| **None**             |                                                  |                                                |

If the host OS has no distro name, "unknown_distro" will be used as the
distro name. If the host OS' distroname is not valid as a var option
name, it will be converted lossily to a valid var option name.

Example:
```yaml
api: v0/package
pkg: example/0.0.1
build:
  auto_host_vars: Distro
  options:
     ...
...
```

## TestSpec

A test spec defines one test script that should be run against the package to validate it. Each test script can run against one stage of the package, meaning that you can define test processes for the source package, build environment (unit tests), or install environment (integration tests).

| Field        | Type                                | Description                                                                                                                        |
| ------------ | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| stage        | _str_                               | The stage that this test validates, one of: **sources**, **build**, **install**                                                    |
| selectors    | _List[[VariantSpec](#variantspec)]_ | Identifies which variants this test should be executed against. Variants must match one of the selectors in this list to be tested |
| requirements | _List[[Request](#request)]_         | Additional packages required in the test environment                                                                               |
| script       | _str_ or _List[str]_                | The sh script which tests the package                                                                                              |

## InstallSpec

| Field        | Type                                    | Description                                                                                                                                                          |
| ------------ | --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| requirements | _List[[Request](#request)]_             | The set of packages required at runtime, this list applies universally to all components.                                                                            |
| embedded     | _List[[Spec](#package-spec)]_           | A list of packages that come bundled in this one                                                                                                                     |
| components   | _List[[ComponentSpec](#componentspec)]_ | The set of components that this package provides. If not otherwise specified, a `build` and `run` component are automatically generated and inserted into this list. |
| environment  | _List[[EnvOp](#envop)]_                 | Environment variable manipulations to make at runtime                                                                                                                |

#### ComponentSpec

The component spec defines a single component of a package. Components can be individually requested for a package. The `build` and `run` components are generated automatically unless they are defined explicitly for a package.

| Field           | Type                                                                    | Description                                                                                                                                             |
| --------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| name            | _string_                                                                | The name of this component                                                                                                                              |
| files           | _List[string]_                                                          | A list of patterns that identify which files belong to this component. Patterns follow the same syntax as gitignore files                               |
| uses            | _List[string]_                                                          | A list of other components from this package that this component uses, and are therefore also included whenever this component is included.             |
| requirements    | _List[[Request](#request)]_                                             | A list of requirements that this component has. These requirements are **in addition to** any requirements defined at the `install.requirements` level. |
| embedded        | _List[[ComponentEmbeddedPackagesSpec](#componentembeddedpackagesspec)]_ | A list of which embedded packages are embedded in this component, and which components of the embedded package are present.                             |
| file_match_mode | _List[[ComponentFileMatchMode](#componentfilematchmode)]_               | Control how the file filters are applied.                                                                                                               |

#### ComponentEmbeddedPackagesSpec

| Value | Description                                                                                                                                                                                                                                                                                                          |
| ----- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| _str_ | A package and component(s), with optional version, in the form of either `pkg-name:comp-name[/version]` or `pkg-name:{comp1,comp2,...,compn}[/version]`, referring to an embedded package and its component(s) defined in the `embedded` section of [InstallSpec](#installspec). At least one component is required. |

#### ComponentFileMatchMode

| Value         | Description                                                                                             |
| ------------- | ------------------------------------------------------------------------------------------------------- |
| All (default) | Matching files are always included                                                                      |
| Remaining     | Matching files are only included if they haven't already been matched by a previously defined component |

### EnvOp

Configurations made to the environment at runtime. Configurations include the environment operations such as [AppendEnv](#appendenv), [PrependEnv](#prependenv), [Comment](#comment) or [SetEnv](#setenv).
Other configuration include setting the priority of the generated activation script. Can be set using [Priority](#priority).

#### AppendEnv

| Field     | Type  | Description                                                                  |
| --------- | ----- | ---------------------------------------------------------------------------- |
| append    | _str_ | The environment variable to append to                                        |
| value     | _str_ | The value to append                                                          |
| separator | _str_ | Optional separator to join with (defaults to `:` on unix and `;` on windows) |

#### PrependEnv

| Field     | Type  | Description                                                                  |
| --------- | ----- | ---------------------------------------------------------------------------- |
| prepend   | _str_ | The environment variable to prepend to                                       |
| value     | _str_ | The value to prepend                                                         |
| separator | _str_ | Optional separator to join with (defaults to `:` on unix and `;` on windows) |

#### SetEnv

| Field | Type  | Description                     |
| ----- | ----- | ------------------------------- |
| set   | _str_ | The environment variable to set |
| value | _str_ | The value to set                |

#### Comment

| Field   | Type  | Description        |
| ------- | ----- | ------------------ |
| comment | _str_ | The comment to add |

#### Priority

| Field    | Type | Description                                                                      |
| -------- | ---- | -------------------------------------------------------------------------------- |
| priority | _u8_ | The priority value to be added onto the filename, only the last priority is used |

### Request

A build option can be one of [VariableRequest](#variablerequest), or [PackageRequest](#packagerequest).

#### VariableRequest

| Field               | Type   | Description                                                                                                                                                                                  |
| ------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| var                 | _str_  | The requested value of a package build variable in the form`name=value`, this can reference a specific package or the global variable (eg `debug=on`, or `python.abi=cp37`)                  |
| fromBuildEnv        | _bool_ | If true, replace the requested value of this variable with the value used in the build environment                                                                                           |
| ifPresentInBuildEnv | _bool_ | Either true or false; if true, then `fromBuildEnv` only applies if the variable was present in the build environment. This allows different variants to have different runtime requirements. |

#### PackageRequest

| Field               | Type                                    | Description                                                                                                                                                                                                     |
| ------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pkg                 | _[`RangeIdentifier`](#rangeidentifier)_ | Specifies a desired package, components and acceptable version range.                                                                                                                                           |
| prereleasePolicy    | _[PreReleasePolicy](#prereleasepolicy)_ | Defines how pre-release versions should be handled when resolving this request                                                                                                                                  |
| inclusionPolicy     | _[InclusionPolicy](#inclusionpolicy)_   | Defines when the requested package should be included in the environment                                                                                                                                        |
| fromBuildEnv        | _str_ or _bool_                         | Either true, or a template to generate this request from using the version of the package that was resolved into the build environment. See [FromBuildEnvTemplate](#frombuildenvtemplate) for more information. |
| ifPresentInBuildEnv | _bool_                                  | Either true or false; if true, then `fromBuildEnv` only applies if the package was present in the build environment. This allows different variants to have different runtime requirements.                     |

##### FromBuildEnvTemplate

This template takes the form of any valid version range expression, but any
_x_ characters that appear are replaced by digits in the version number. For
example, if `python/2.7.5` is in the build environment, the template `~x.x`
would become `~2.7`. The special values of `Binary` and `API` can be used to
request a binary or API compatible package to the one in the build environment,
respectively. For Example, if `mypkg/1.2.3.4` is in the build environment, the
template `API` would become `API:1.2.3.4`. A value of `true` works the same as
`Binary`.

###### Advanced Usage

Besides `x`, other characters are available:

- 'v' - Expands to the full base version of the package.
- 'V' - Expands to the full version of the package, including any pre/post
  release information.
- 'X' - Expands all the pre or post release information, depending on its
  position in the template. It's okay if the package does not have any pre or
  post release components.

Examples:

If the target package is `python/3.9.5-alpha.1+post.1,hotfix.2`, then:

- `~x.x` -> `~3.9`
- `~v` -> `~3.9.5`
- `~V` -> `~3.9.5-alpha.1+post.1,hotfix.2`
- `~x.x-X` -> `~3.9-alpha.1`
- `~x.x+X` -> `~3.9+hotfix.2,post.1`
- `~x.x-X+X` -> `~3.9-alpha1+hotfix.2,post.1`

#### RangeIdentifier

Like an [Identifier](#identifier) but with a version range rather than an exact version, see [versioning]({{< ref "../../../use/versioning" >}}). Additionally, range identifiers can be used to identify one or more package components. The `name:component` syntax can be used when only one component is desired, and the `:{component,component}` syntax for when multiple are desired:

```txt
mypkg:lib/1.0.0
mypkg:dev/1.0.0
mypkg:debug/1.0.0
mypkg:{lib,dev}/1.0.0
mypkg:{lib,dev,debug}/1.0.0
```

#### PreReleasePolicy

| Value                | Description                                 |
| -------------------- | ------------------------------------------- |
| ExcludeAll (default) | Do not include pre-release package versions |
| IncludeAll           | Include all pre-release package versions    |

#### InclusionPolicy

| Value            | Description                                                                                                                                   |
| ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Always (default) | Always include the requested package in the environment                                                                                       |
| IfAlreadyPresent | Only include this package in the environment if it is already in the environment or another request exists with the `Always` inclusion policy |

## Identifier

The package identifier takes the form `<name>[/<version>[/<build>]]`, where:

| Component | Description                                                                                                                                                                                                                                                                       |
| --------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| name      | The package name, can only have lowercase letter and dashes (`-`)                                                                                                                                                                                                                 |
| version   | The version number, see [versioning]({{< ref "../../../use/versioning" >}})                                                                                                                                                                                                       |
| build     | The build string, should not be specified in a spec file as it is generated by the system at build time. Digests are calculated based on the package build options, and there are two special values `src` and `embedded` for source packages and embedded packages, respectively |

## Compat

Specifies the compatibility contract of a version number. The compat string is a dot-separated set of characters that define contract, for example `x.a.b` (the default contract) says that major version changes are not compatible, minor version changes provides **A**PI compatibility, and patch version changes provide **B**inary compatibility.
