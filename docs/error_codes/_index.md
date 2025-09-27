---
title: Error Codes and Debugging
summary: Application error code documentation and help
weight: 999
---

Both spfs and spk have defined error codes that are produced. These codes can be looked up on this page for help understanding and debugging them.

## Spk Errors

### Build Validation Errors

These rules are checked either before or just after a package build and validate the build setup and collected package contents for common issues and potential danger. In all cases it will depend on the default and any additional configuration of the [validation rules]({{< ref "../ref/api/v0/package" >}}#validationspec) in the package spec.

#### `spk::build::validation::empty_package`

This validation is triggered when the build creates a package that has no files in it and is denied by default. This is typically caused by a mistake or error in the build script.

Recipes for some special packages may also require that they create an empty package, which will fail with different wording if files are installed.

#### `spk::build::validation::collect_all_files`

This validation is triggered when the build script installs files to the spfs area but then does not collect them as part of one of its components. The default package components will collect all files, but if custom components are defined each with a subset of files, then each file must be collected by at least one of them. Typically, leaving files behind is a sign that the recipe has forgotten to identify some, and doing so could create a broken package with only some if the expected files.

#### `spk::build::validation::alter_existing_files`

This validation is triggered when the build process modifies the files of another package during the build process and is denied by default. Doing so may cause the files to become part of the new package which is typically not desired and often points to an issue in the build script.

If you cannot stop the build process from modifying the files, you can add a call to `spfs reset <PATHS>` at the end of the build script to undo any changes to the affected paths.

#### `spk::build::validation::collect_existing_files`

This validation is triggered when the created package includes files that belonged to another package in the build environment, and is denied by default. Typically, if a package needs files from another one, it should be listed as a dependency rather than collected directly into the package. This will happen if your build script writes to any of the files from other packages, even if the file contents end up exactly the same as before.

If you cannot stop the build process from touching the files, you can add a call to `spfs reset <PATHS>` at the end of the build script to undo any changes to the affected paths.

#### `spk::build::validation::inherited_requirement`

Packages that appear in a build environment may assert that one or more requirements must be included in the output package. Typically, this is because building against that dependency crates a compatibility requirement that should be represented in your package and should be added as denoted by the error message.

In some cases, the developer maybe know that the dependency was not used for compilation or the compatibility requirement was somehow mitigated because of how it was being used in the build environment. In these cases, the validation rule can be disabled like any other (see [validation rules]({{< ref "../ref/api/v0/package" >}}#validationspec))

#### `spk::build::validation::recursive_build`

This validation is triggered when a version of the package being built appears in the resolved build environment (either directly or indirectly as a dependency of a dependency). Typically, this is not desired and creates confusing build output with other errors.

In the case that this is expected and desired, see the documentation section on [recursive builds]({{< ref "../use/create/recursive_builds" >}})

#### `spk::build::validation::long_var_description`

This validation is triggered when a build var description is greater than 256 characters is found.

In cases where a longer description is required, see the documentation section on [build variable description]({{< ref "../use/create/build" >}}#buildvariabledescription)

#### `spk::build::validation::strong_inheritance_var_description`

This validation is triggered when a description is not provided for strong inheritance build variables.

In cases where a description is not required, see the documentation section on [build variable description]({{< ref "../use/create/build" >}}#buildvariabledescription)

#### `spk::build::validation::spdx_license`

This validation is triggered when a valid spdx license is not provided. By default, a license is not required, but when given it must be from the [SPDX License List](https://spdx.org/licenses/).

In cases where a custom license string is needed, reverse the default validation like so:

```yaml
...

build:
  validation:
    - deny: SpdxLicense
...
```

## Spfs Errors

### `spfs::generic`

This is a generic error code that has no more specific information or help documentation attached. If you encounter one of these, please reach out for help by submitting an issue on [github](https://github.com/spkenv/spk).

### `spfs::could_not_create_spfs_dir`

Spfs relies on a specific directory in which to work. All files in the runtime environment are visible at that location and this root directory must exist before spfs can use it.

Normally, our provided system packages will create this directory for you. If you have not used one of the installers, or the directory was subsequently removed, spk will still try to create the directory the next time it runs. You will see this error whenever it fails to create the directory. It is a critical error as spfs cannot continue without it being present.

Possible resolutions:

- Reinstall spk/spfs using one of our provided packages
- Create the directory, or have your system administrator create the required directory for you

### `spfs::unknown_remote`

Spfs has one local repository to store data, and any number of _remote_ ones. These are either configured in the spfs config file, or specified at the command line. An unknown remote error occurs when a remote was specified, but a remote with that name does not appear in the spfs config file.

You can check the currently configured set of remotes with the `spfs config` command.

Possible resolutions:

- Check the spelling of the remote
- If you mean for the remote to be specified on the command line, ensure that it follows a url format
- Check the [spfs config]({{< ref "../admin/config" >}}) documentation
- Contact your system administrator

### `spfs::failed_to_open_repo`

This error occurs when a remote repository could not be opened/connected to in order to read/write spfs data. This can happen for a number of reasons, and usually specifies an additional cause, often one of the errors below:

<!-- OpenRepositoryError  -->

#### `spfs::storage::fs::not_initialized`

By default, filesystem-based repositories in spfs need to be correctly initialized before they can be used. This error occurs when a filesystem-based repository path in the config or specified on the command line does not exist or has not been setup as such.

Possible resolutions:

- Check that the path is valid and inputted correctly
- Use the `spfs init repo` command to initialize the repository

#### `spfs::storage::invalid_query`

Spfs repositories can be configured using an address/url. Many repository types, however, require additional settings to be specified in the url query. This error occurs when one or more of those query parameters is missing, incorrect, or otherwise invalid.

Possible resolutions:

- Check the error message for information about which part(s) were invalid
- Check the [spfs config]({{< ref "../admin/config" >}}) documentation for information on the url formats

#### `spfs::storage::missing_query`

Spfs repositories can be configured using an address/url. Many repository types, however, require additional settings to be specified in the url query. This error occurs when one or more parameters was required but the query was missing entirely.

Possible resolutions:

- Add a `?` to the end of the url and try again
- Check the [spfs config]({{< ref "../admin/config" >}}) documentation for information on the url formats
