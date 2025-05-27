---
title: SPFS CLI
chapter: true
---

# Command-Line Help for `spfs-cli-main`

This document contains the help content for the `spfs-cli-main` command-line program.

**Command Overview:**

* [`spfs-cli-main`Γå┤](#spfs-cli-main)
* [`spfs-cli-main version`Γå┤](#spfs-cli-main-version)
* [`spfs-cli-main init`Γå┤](#spfs-cli-main-init)
* [`spfs-cli-main init repo`Γå┤](#spfs-cli-main-init-repo)
* [`spfs-cli-main edit`Γå┤](#spfs-cli-main-edit)
* [`spfs-cli-main commit`Γå┤](#spfs-cli-main-commit)
* [`spfs-cli-main config`Γå┤](#spfs-cli-main-config)
* [`spfs-cli-main reset`Γå┤](#spfs-cli-main-reset)
* [`spfs-cli-main run`Γå┤](#spfs-cli-main-run)
* [`spfs-cli-main tag`Γå┤](#spfs-cli-main-tag)
* [`spfs-cli-main untag`Γå┤](#spfs-cli-main-untag)
* [`spfs-cli-main shell`Γå┤](#spfs-cli-main-shell)
* [`spfs-cli-main runtime`Γå┤](#spfs-cli-main-runtime)
* [`spfs-cli-main runtime info`Γå┤](#spfs-cli-main-runtime-info)
* [`spfs-cli-main runtime list`Γå┤](#spfs-cli-main-runtime-list)
* [`spfs-cli-main runtime prune`Γå┤](#spfs-cli-main-runtime-prune)
* [`spfs-cli-main runtime remove`Γå┤](#spfs-cli-main-runtime-remove)
* [`spfs-cli-main layers`Γå┤](#spfs-cli-main-layers)
* [`spfs-cli-main platforms`Γå┤](#spfs-cli-main-platforms)
* [`spfs-cli-main tags`Γå┤](#spfs-cli-main-tags)
* [`spfs-cli-main info`Γå┤](#spfs-cli-main-info)
* [`spfs-cli-main pull`Γå┤](#spfs-cli-main-pull)
* [`spfs-cli-main push`Γå┤](#spfs-cli-main-push)
* [`spfs-cli-main log`Γå┤](#spfs-cli-main-log)
* [`spfs-cli-main search`Γå┤](#spfs-cli-main-search)
* [`spfs-cli-main diff`Γå┤](#spfs-cli-main-diff)
* [`spfs-cli-main ls-tags`Γå┤](#spfs-cli-main-ls-tags)
* [`spfs-cli-main ls`Γå┤](#spfs-cli-main-ls)
* [`spfs-cli-main migrate`Γå┤](#spfs-cli-main-migrate)
* [`spfs-cli-main check`Γå┤](#spfs-cli-main-check)
* [`spfs-cli-main read`Γå┤](#spfs-cli-main-read)
* [`spfs-cli-main write`Γå┤](#spfs-cli-main-write)
* [`spfs-cli-main docs`Γå┤](#spfs-cli-main-docs)

## `spfs-cli-main`

SPK is a Package Manager for high-velocity software environments, built on SPFS. SPFS is a system for filesystem isolation, capture, and distribution.

**Usage:** `spfs-cli-main [OPTIONS] <COMMAND>`

EXTERNAL SUBCOMMANDS:
    render       render the contents of an environment or layer
    monitor      watch a runtime and clean it up when complete

###### **Subcommands:**

* `version` ΓÇö Print the version of spfs
* `init` ΓÇö Create an empty filesystem repository
* `edit` ΓÇö Make the current runtime editable
* `commit` ΓÇö Commit the current runtime state or a directory to storage
* `config` ΓÇö Output the current configuration of spfs
* `reset` ΓÇö Reset changes, or rebuild the entire spfs directory
* `run` ΓÇö Run a program in a configured spfs environment
* `tag` ΓÇö Tag an object
* `untag` ΓÇö Remove tag versions or entire tag streams
* `shell` ΓÇö Enter a subshell in a configured spfs environment
* `runtime` ΓÇö View and manage spfs runtime information
* `layers` ΓÇö List all layers in an spfs repository
* `platforms` ΓÇö List all platforms in an spfs repository
* `tags` ΓÇö List all tags in an spfs repository
* `info` ΓÇö Display information about the current environment, or specific items
* `pull` ΓÇö Pull one or more objects to the local repository
* `push` ΓÇö Push one or more objects to a remote repository
* `log` ΓÇö Log the history of a given tag over time
* `search` ΓÇö Search for available tags by substring
* `diff` ΓÇö Compare two spfs file system states
* `ls-tags` ΓÇö List tags by their path
* `ls` ΓÇö List the contents of a committed directory
* `migrate` ΓÇö Migrate the data from and older repository format to the latest one
* `check` ΓÇö Check a repositories internal integrity
* `read` ΓÇö Output the contents of a blob to stdout
* `write` ΓÇö Store an arbitrary blob of data in spfs
* `docs` ΓÇö Output the current configuration of spfs

###### **Options:**

* `-v`, `--verbose` ΓÇö Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` ΓÇö Additionally log output to the provided file
* `--timestamp` ΓÇö Enables timestamp in logging (always enabled in file log)



## `spfs-cli-main version`

Print the version of spfs

**Usage:** `spfs-cli-main version`



## `spfs-cli-main init`

Create an empty filesystem repository

**Usage:** `spfs-cli-main init <COMMAND>`

###### **Subcommands:**

* `repo` ΓÇö Initialize an empty filesystem repository



## `spfs-cli-main init repo`

Initialize an empty filesystem repository

Does nothing when run on an existing repository

**Usage:** `spfs-cli-main init repo <PATH>`

###### **Arguments:**

* `<PATH>` ΓÇö The root of the new repository



## `spfs-cli-main edit`

Make the current runtime editable

**Usage:** `spfs-cli-main edit [OPTIONS]`

###### **Options:**

* `--off` ΓÇö Disable edit mode instead
* `--keep-runtime` ΓÇö Change a runtime into a durable runtime, will also make the runtime editable



## `spfs-cli-main commit`

Commit the current runtime state or a directory to storage

**Usage:** `spfs-cli-main commit [OPTIONS] [KIND]`

###### **Arguments:**

* `<KIND>` ΓÇö The desired object type to create, skip this when giving --path or --ref

  Possible values: `layer`, `platform`


###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Commit files directly into a remote repository

   The default is to commit to the local repository. This flag is only valid with the --path argument.
* `-t`, `--tag <TAGS>` ΓÇö A human-readable tag for the generated object

   Can be provided more than once.
* `--path <PATH>` ΓÇö Commit this directory instead of the current spfs changes
* `--ref <REFERENCE>` ΓÇö Combine existing items into a platform, use a '+' to join multiple
* `--hash-while-committing` ΓÇö Hash the files while committing, rather than before.

   This option can improve commit times when a large number of the files are both large, and don't already exist in the repository. It may degrade commit times when committing directly to a slow or remote repository. When given, all files are written to the repository even if the payload exists, rather than hashing the file first to determine if it needs to be transferred.
* `--max-concurrent-blobs <MAX_CONCURRENT_BLOBS>` ΓÇö The total number of blobs that can be committed concurrently

  Default value: `1000`
* `--max-concurrent-branches <MAX_CONCURRENT_BRANCHES>` ΓÇö The total number of branches that can be processed concurrently at each level of the rendered file tree.

   The number of active trees being processed can grow exponentially by this exponent for each additional level of depth in the rendered file tree. In general, this number should be kept low.

  Default value: `5`



## `spfs-cli-main config`

Output the current configuration of spfs

**Usage:** `spfs-cli-main config`



## `spfs-cli-main reset`

Reset changes, or rebuild the entire spfs directory

**Usage:** `spfs-cli-main reset [OPTIONS] [PATHS]...`

###### **Arguments:**

* `<PATHS>` ΓÇö Glob patterns in the spfs dir of files to reset, defaults to everything

###### **Options:**

* `--sync` ΓÇö Sync the latest information for each tag even if it already exists
* `--check` ΓÇö Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` ΓÇö Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` ΓÇö The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` ΓÇö The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` ΓÇö Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-e`, `--edit` ΓÇö Mount the resulting runtime in edit mode

   Default to true if REF is empty or not given
* `-r`, `--ref <REFERENCE>` ΓÇö The tag or id to rebuild the runtime with.

   Uses current runtime stack if not given. Use '-' or an empty string to request an empty environment. Only valid if no paths are given



## `spfs-cli-main run`

Run a program in a configured spfs environment

**Usage:** `spfs-cli-main run [OPTIONS] <--rerun <RUNTIME_NAME>|REFERENCE> [-- <COMMAND>...]`

###### **Arguments:**

* `<REFERENCE>` ΓÇö The tag or id of the desired runtime

   Use '-' to request an empty environment
* `<COMMAND>` ΓÇö The command to run in the environment and its arguments

   In order to ensure that flags are passed as-is, '--' must be place before specifying the command and any flags that should be given to that command: e.g. `spfs run <args> -- command --flag-for-command`

###### **Options:**

* `--sync` ΓÇö Sync the latest information for each tag even if it already exists
* `--check` ΓÇö Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` ΓÇö Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` ΓÇö The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` ΓÇö The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` ΓÇö Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose` ΓÇö Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` ΓÇö Additionally log output to the provided file
* `--timestamp` ΓÇö Enables timestamp in logging (always enabled in file log)
* `-e`, `--edit` ΓÇö Mount the spfs filesystem in edit mode (default if REF is empty or not given)
* `--no-edit` ΓÇö Mount the spfs filesystem in read-only mode (default if REF is non-empty)
* `--force` ΓÇö Requires --rerun. Force reset the process fields of the runtime before it is run again
* `-k`, `--keep-runtime` ΓÇö Use to keep the runtime around rather than deleting it when the process exits. This is best used with '--name NAME' to make rerunning the runtime easier at a later time
* `--runtime-name <RUNTIME_NAME>` ΓÇö Provide a name for this runtime to make it easier to identify
* `--rerun <RUNTIME_NAME>` ΓÇö Name of an existing durable runtime to reuse for this run
* `--annotation <KEY:VALUE>` ΓÇö Adds annotation key-value string data to the new runtime.

   This allows external processes to store arbitrary data in the runtimes they create. This is most useful with durable runtimes. The data can be retrieved by running `spfs runtime info` or `spfs info` and using the `--get <KEY>` or `--get-all` options

   Annotation data is specified as key-value string pairs separated by either an equals sign or colon (--annotation name=value --annotation other:value). Multiple pairs of annotation data can also be specified at once in yaml or json format (--annotation '{name: value, other: value}').

   Annotation data can also be given in a json or yaml file, by using the `--annotation-file <FILE>` argument. If given, `--annotation` arguments will supersede anything given in annotation files.

   If the same key is used more than once, the last key-value pair will override the earlier values for the same key.
* `--annotation-file <ANNOTATION_FILE>` ΓÇö Specify annotation key-value data from a json or yaml file (see --annotation)



## `spfs-cli-main tag`

Tag an object

**Usage:** `spfs-cli-main tag [OPTIONS] <TARGET_REF> <TAG>...`

###### **Arguments:**

* `<TARGET_REF>` ΓÇö The reference or id of the item to tag
* `<TAG>` ΓÇö The tag(s) to point to the the given target

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Create tags in a remote repository instead of the local one



## `spfs-cli-main untag`

Remove tag versions or entire tag streams

**Usage:** `spfs-cli-main untag [OPTIONS] <TAG>`

###### **Arguments:**

* `<TAG>` ΓÇö The tag to remove

   Unless --all or --latest is provided, this must have an explicit version number (eg: path/name~0)

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Remove tags in a remote repository instead of the local one
* `--latest` ΓÇö Only remove the latest version of this tag
* `-a`, `--all` ΓÇö Remove all versions of this tag, deleting it completely



## `spfs-cli-main shell`

Enter a subshell in a configured spfs environment

**Usage:** `spfs-cli-main shell [OPTIONS] <--rerun <RUNTIME_NAME>|REF>`

###### **Arguments:**

* `<REF>` ΓÇö The tag or id of the desired runtime

   Use '-' or nothing to request an empty environment

###### **Options:**

* `--sync` ΓÇö Sync the latest information for each tag even if it already exists
* `--check` ΓÇö Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` ΓÇö Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` ΓÇö The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` ΓÇö The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` ΓÇö Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose` ΓÇö Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` ΓÇö Additionally log output to the provided file
* `--timestamp` ΓÇö Enables timestamp in logging (always enabled in file log)
* `-e`, `--edit` ΓÇö Mount the spfs filesystem in edit mode (true if REF is empty or not given)
* `--no-edit` ΓÇö Mount the spfs filesystem in read-only mode (default if REF is non-empty)
* `--rerun <RUNTIME_NAME>` ΓÇö Name of a previously run durable runtime to reuse for this run
* `--force` ΓÇö Requires --rerun. Force reset the process fields of the runtime before it is run again
* `--runtime-name <RUNTIME_NAME>` ΓÇö Provide a name for this runtime to make it easier to identify
* `-k`, `--keep-runtime` ΓÇö Use to keep the runtime around rather than deleting it when the process exits. This is best used with '--name NAME' to make rerunning the runtime easier at a later time
* `--annotation <KEY:VALUE>` ΓÇö Adds annotation key-value string data to the new runtime.

   This allows external processes to store arbitrary data in the runtimes they create. This is most useful with durable runtimes. The data can be retrieved by running `spfs runtime info` or `spfs info` and using the `--get <KEY>` or `--get-all` options

   Annotation data is specified as key-value string pairs separated by either an equals sign or colon (--annotation name=value --annotation other:value). Multiple pairs of annotation data can also be specified at once in yaml or json format (--annotation '{name: value, other: value}').

   Annotation data can also be given in a json or yaml file, by using the `--annotation-file <FILE>` argument. If given, `--annotation` arguments will supersede anything given in annotation files.

   If the same key is used more than once, the last key-value pair will override the earlier values for the same key.
* `--annotation-file <ANNOTATION_FILE>` ΓÇö Specify annotation key-value data from a json or yaml file (see --annotation)



## `spfs-cli-main runtime`

View and manage spfs runtime information

**Usage:** `spfs-cli-main runtime <COMMAND>`

**Command Alias:** `rt`

###### **Subcommands:**

* `info` ΓÇö Show the complete state of a runtime
* `list` ΓÇö List runtime information from the repository
* `prune` ΓÇö Find and remove runtimes from the repository based on a pruning strategy
* `remove` ΓÇö Remove runtimes from the repository



## `spfs-cli-main runtime info`

Show the complete state of a runtime

**Usage:** `spfs-cli-main runtime info [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` ΓÇö The name/id of the runtime to remove

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Load a runtime in a remote or alternate repository
* `--get <GET>` ΓÇö Output the data value for the given annotation key(s) from the active runtime. Each value is printed on its own line without its key
* `--get-all` ΓÇö Output all the annotation keys and values from the active runtime as a yaml dictionary



## `spfs-cli-main runtime list`

List runtime information from the repository

**Usage:** `spfs-cli-main runtime list [OPTIONS]`

**Command Alias:** `ls`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö List runtimes in a remote or alternate repository
* `-q`, `--quiet` ΓÇö Only print the name of each runtime, no additional data



## `spfs-cli-main runtime prune`

Find and remove runtimes from the repository based on a pruning strategy

**Usage:** `spfs-cli-main runtime prune [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Prune a runtime in a remote or alternate repository
* `--ignore-user` ΓÇö Remove the runtime even if it's owned by someone else
* `--ignore-host` ΓÇö Remove the runtime even if it appears to be from a different host

   Implies --ignore-monitor
* `--ignore-monitor` ΓÇö Do not try and terminate the monitor process, just remove runtime data
* `--from-before-boot` ΓÇö Remove runtimes started before last reboot



## `spfs-cli-main runtime remove`

Remove runtimes from the repository

**Usage:** `spfs-cli-main runtime remove [OPTIONS] [NAME]...`

**Command Alias:** `rm`

###### **Arguments:**

* `<NAME>` ΓÇö The name/id of the runtime to remove

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Remove a runtime in a remote or alternate repository
* `-f`, `--force` ΓÇö Remove the runtime from the repository forcefully

   Even if the monitor cannot be stopped or killed the data will be removed from the repository.
* `--ignore-user` ΓÇö Remove the runtime even if it's owned by someone else
* `--ignore-host` ΓÇö Remove the runtime even if it appears to be from a different host

   Implies --ignore-monitor
* `--ignore-monitor` ΓÇö Do not try and terminate the monitor process, just remove runtime data
* `--remove-durable` ΓÇö Allow durable runtimes to be removed, normally they will not be removed



## `spfs-cli-main layers`

List all layers in an spfs repository

**Usage:** `spfs-cli-main layers [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Show layers from remote repository instead of the local one
* `--short` ΓÇö Show the shortened form of each reported layer digest
* `--tags` ΓÇö Also find and report any tags that point to each layer, implies --short



## `spfs-cli-main platforms`

List all platforms in an spfs repository

**Usage:** `spfs-cli-main platforms [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Show layers from remote repository instead of the local one
* `--short` ΓÇö Show the shortened form of each reported layer digest
* `--tags` ΓÇö Also find and report any tags that point to each platform, implies --short



## `spfs-cli-main tags`

List all tags in an spfs repository

**Usage:** `spfs-cli-main tags [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Show layers from remote repository instead of the local one
* `--target` ΓÇö Also show the target digest of each tag
* `--short` ΓÇö Show the shortened form of each reported digest, implies --target



## `spfs-cli-main info`

Display information about the current environment, or specific items

**Usage:** `spfs-cli-main info [OPTIONS] [REF]...`

###### **Arguments:**

* `<REF>` ΓÇö Tag, id, or /spfs/file/path to show information about

###### **Options:**

* `-v`, `--verbose` ΓÇö Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` ΓÇö Additionally log output to the provided file
* `--timestamp` ΓÇö Enables timestamp in logging (always enabled in file log)
* `--get <GET>` ΓÇö Output the data value for the given annotation key(s) from the active runtime. Each value is printed on its own line without its key
* `--get-all` ΓÇö Output all the annotation keys and values from the active runtime as a yaml dictionary
* `-H`, `--human-readable` ΓÇö Lists file sizes in human readable format
* `-r`, `--remote <REMOTE>` ΓÇö Operate on a remote repository instead of the local one

   This is really only helpful if you are providing a specific ref to look up.
* `--tags` ΓÇö Also find and report any tags that point to any identified digest (implies '--short')
* `--short` ΓÇö Use shortened digests in the output (nicer, but slower)
* `--follow` ΓÇö Follow and show child objects, depth-first



## `spfs-cli-main pull`

Pull one or more objects to the local repository

**Usage:** `spfs-cli-main pull [OPTIONS] <REF>...`

###### **Arguments:**

* `<REF>` ΓÇö The reference(s) to pull/localize

   These can be individual tags or digests, or they may also be a collection of items joined by a '+'

###### **Options:**

* `--sync` ΓÇö Sync the latest information for each tag even if it already exists
* `--check` ΓÇö Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` ΓÇö Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` ΓÇö The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` ΓÇö The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` ΓÇö Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose`
* `-r`, `--remote <REMOTE>` ΓÇö The name or address of the remote server to pull from

   Defaults to searching all configured remotes



## `spfs-cli-main push`

Push one or more objects to a remote repository

**Usage:** `spfs-cli-main push [OPTIONS] <REF>...`

###### **Arguments:**

* `<REF>` ΓÇö The reference(s) to push

   These can be individual tags or digests, or they may also be a collection of items joined by a '+'

###### **Options:**

* `--sync` ΓÇö Sync the latest information for each tag even if it already exists
* `--check` ΓÇö Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` ΓÇö Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` ΓÇö The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` ΓÇö The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` ΓÇö Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose`
* `-r`, `--remote <REMOTE>` ΓÇö The name or address of the remote server to push to

  Default value: `origin`



## `spfs-cli-main log`

Log the history of a given tag over time

**Usage:** `spfs-cli-main log [OPTIONS] <TAG>`

###### **Arguments:**

* `<TAG>` ΓÇö The tag to show history of

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Load the tag from remote repository instead of the local one



## `spfs-cli-main search`

Search for available tags by substring

**Usage:** `spfs-cli-main search <TERM>`

###### **Arguments:**

* `<TERM>` ΓÇö The search term/substring to look for



## `spfs-cli-main diff`

Compare two spfs file system states

**Usage:** `spfs-cli-main diff [FROM] [TO]`

###### **Arguments:**

* `<FROM>` ΓÇö The tag or id to use as the base of the computed diff, defaults to the current runtime
* `<TO>` ΓÇö The tag or id to diff the base against, defaults to the contents of the spfs filesystem



## `spfs-cli-main ls-tags`

List tags by their path

**Usage:** `spfs-cli-main ls-tags [OPTIONS] [PATH]`

**Command Alias:** `list-tags`

###### **Arguments:**

* `<PATH>` ΓÇö The tag path to list under

  Default value: `/`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö List tags from a remote repository instead of the local one
* `--recursive` ΓÇö Walk the tag tree recursively listing all tags under the specified dir



## `spfs-cli-main ls`

List the contents of a committed directory

**Usage:** `spfs-cli-main ls [OPTIONS] <REF> [PATH]`

**Command Aliases:** `list-dir`, `list`

###### **Arguments:**

* `<REF>` ΓÇö The tag or digest of the file tree to read from
* `<PATH>` ΓÇö The subdirectory to list

  Default value: `/spfs`

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö List files on a remote repository instead of the local one
* `-R`, `--recursive` ΓÇö Recursively list all files and directories
* `-l` ΓÇö Long listing format
* `-H`, `--human-readable` ΓÇö Lists file sizes in human readable format



## `spfs-cli-main migrate`

Migrate the data from and older repository format to the latest one

**Usage:** `spfs-cli-main migrate [OPTIONS] <PATH>`

###### **Arguments:**

* `<PATH>` ΓÇö The path to the filesystem repository to migrate

###### **Options:**

* `--upgrade` ΓÇö Replace old data with migrated data one complete



## `spfs-cli-main check`

Check a repositories internal integrity

**Usage:** `spfs-cli-main check [OPTIONS] [REF]...`

###### **Arguments:**

* `<REF>` ΓÇö Objects to recursively check, defaults to everything

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Trigger the check operation on a remote repository instead of the local one
* `--max-tag-stream-concurrency <MAX_TAG_STREAM_CONCURRENCY>` ΓÇö The maximum number of tag streams that can be read and processed at once

  Default value: `1000`
* `--max-object-concurrency <MAX_OBJECT_CONCURRENCY>` ΓÇö The maximum number of objects that can be validated at once

  Default value: `5000`
* `--pull <PULL>` ΓÇö Attempt to fix problems by pulling from another repository. Defaults to "origin"



## `spfs-cli-main read`

Output the contents of a blob to stdout

**Usage:** `spfs-cli-main read [OPTIONS] <REF> [PATH]`

**Command Aliases:** `read-file`, `cat`, `cat-file`

###### **Arguments:**

* `<REF>` ΓÇö The tag or digest of the blob/payload to output
* `<PATH>` ΓÇö If the given ref is not a blob, read the blob found at this path

###### **Options:**

* `-r`, `--remote <REMOTE>` ΓÇö Read from a remote repository instead of the local one



## `spfs-cli-main write`

Store an arbitrary blob of data in spfs

**Usage:** `spfs-cli-main write [OPTIONS]`

**Command Alias:** `write-file`

###### **Options:**

* `-t`, `--tag <TAGS>` ΓÇö A human-readable tag for the generated object

   Can be provided more than once.
* `-r`, `--remote <REMOTE>` ΓÇö Write to a remote repository instead of the local one
* `-f`, `--file <FILE>` ΓÇö Store the contents of this file instead of reading from stdin



## `spfs-cli-main docs`

Output the current configuration of spfs

**Usage:** `spfs-cli-main docs`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

