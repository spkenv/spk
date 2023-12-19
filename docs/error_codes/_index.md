---
title: Error Codes and Debugging
summary: Application error code documentation and help
weight: 999
---

## Spfs Errors

### spfs::generic

This is a generic error code that has no more specific information or help documentation attached. If you encounter one of these, please reach out for help by submitting an issue on [github](https://github.com/imageworks/spk).

### spfs::could_not_create_spfs_dir

Spfs relies on a specific directory in which to work. All files in the runtime environment are visible at that location and this root directory must exist before spfs can use it.

Normally, our provided system packages will create this directory for you. If you have not used one of the installers, or the directory was subsequently removed, spk will still try to create the directory the next time it runs. You will see this error whenever it fails to create the directory. It is a critical error as spfs cannot continue without it being present.

Possible resolutions:

- Reinstall spk/spfs using one of our provided packages
- Create the directory, or have your system administrator create the required directory for you

### spfs::unknown_remote

Spfs has one local repository to store data, and any number of _remote_ ones. These are either configured in the spfs config file, or specified at the command line. An unknown remote error occurs when a remote was specified, but a remote with that name does not appear in the spfs config file.

You can check the currently configured set of remotes with the `spfs config` command.

Possible resolutions:

- Check the spelling of the remote
- If you mean for the remote to be specified on the command line, ensure that it follows a url format
- Check the [spfs config]({{< ref "../spfs/configuration" >}}) documentation
- Contact your system administrator

### spfs::failed_to_open_repo

This error occurs when a remote repository could not be opened/connected to in order to read/write spfs data. This can happen for a number of reasons, and usually specifies an additional cause, often one of the errors below:

<!-- OpenRepositoryError  -->

#### spfs::storage::fs::not_initialized

By default, filesystem-based repositories in spfs need to be correctly initialized before they can be used. This error occurs when a filesystem-based repository path in the config or specified on the command line does not exist or has not been setup as such.

Possible resolutions:

- Check that the path is valid and inputted correctly
- Use the `spfs init repo` command to initialize the repository

#### spfs::storage::invalid_query

Spfs repositories can be configured using an address/url. Many repository types, however, require additional settings to be specified in the url query. This error occurs when one or more of those query parameters is missing, incorrect, or otherwise invalid.

Possible resolutions:

- Check the error message for information about which part(s) were invalid
- Check the [spfs config]({{< ref "../spfs/configuration" >}}) documentation for information on the url formats

#### spfs::storage::missing_query

Spfs repositories can be configured using an address/url. Many repository types, however, require additional settings to be specified in the url query. This error occurs when one or more parameters was required but the query was missing entirely.

Possible resolutions:

- Add a `?` to the end of the url and try again
- Check the [spfs config]({{< ref "../spfs/configuration" >}}) documentation for information on the url formats


