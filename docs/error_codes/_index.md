---
title: Error Codes and Debugging
summary: Application error code documentation and help
weight: 999
---

## spfs::could_not_create_spfs_dir

Spfs relies on a specific directory in which to work. All files in the runtime environment are visible at that location and this root directory must exist before spfs can use it.

Normally, our provided system packages will create this directory for you. If you have not used one of the installers, or the directory was subsequently removed, spk will still try to create the directory the next time it runs. You will see this error whenever it fails to create the directory. It is a critical error as spfs cannot continue without it being present.

Possible resolutions:

- Reinstall spk/spfs using one of our provided packages
- Create the directory, or have your system administrator create the required directory for you

## spfs::generic

This is a generic error code that has no more specific information or help documentation attached. If you encounter one of these, please reach out for help by submitting an issue on [github](https://github.com/imageworks/spk).

## spfs::unknown_remote
