---
title: SPFS CLI
chapter: true
---

# Command-Line Help for `spfs-cli-main`

This document contains the help content for the `spfs-cli-main` command-line program.

**Command Overview:**

* [`spfs-cli-main`↴](#spfs-cli-main)
* [`spfs-cli-main version`↴](#spfs-cli-main-version)
* [`spfs-cli-main init`↴](#spfs-cli-main-init)
* [`spfs-cli-main init repo`↴](#spfs-cli-main-init-repo)
* [`spfs-cli-main edit`↴](#spfs-cli-main-edit)
* [`spfs-cli-main commit`↴](#spfs-cli-main-commit)
* [`spfs-cli-main config`↴](#spfs-cli-main-config)
* [`spfs-cli-main reset`↴](#spfs-cli-main-reset)
* [`spfs-cli-main run`↴](#spfs-cli-main-run)
* [`spfs-cli-main tag`↴](#spfs-cli-main-tag)
* [`spfs-cli-main untag`↴](#spfs-cli-main-untag)
* [`spfs-cli-main shell`↴](#spfs-cli-main-shell)
* [`spfs-cli-main runtime`↴](#spfs-cli-main-runtime)
* [`spfs-cli-main runtime info`↴](#spfs-cli-main-runtime-info)
* [`spfs-cli-main runtime list`↴](#spfs-cli-main-runtime-list)
* [`spfs-cli-main runtime prune`↴](#spfs-cli-main-runtime-prune)
* [`spfs-cli-main runtime remove`↴](#spfs-cli-main-runtime-remove)
* [`spfs-cli-main layers`↴](#spfs-cli-main-layers)
* [`spfs-cli-main platforms`↴](#spfs-cli-main-platforms)
* [`spfs-cli-main tags`↴](#spfs-cli-main-tags)
* [`spfs-cli-main info`↴](#spfs-cli-main-info)
* [`spfs-cli-main pull`↴](#spfs-cli-main-pull)
* [`spfs-cli-main push`↴](#spfs-cli-main-push)
* [`spfs-cli-main log`↴](#spfs-cli-main-log)
* [`spfs-cli-main search`↴](#spfs-cli-main-search)
* [`spfs-cli-main diff`↴](#spfs-cli-main-diff)
* [`spfs-cli-main ls-tags`↴](#spfs-cli-main-ls-tags)
* [`spfs-cli-main ls`↴](#spfs-cli-main-ls)
* [`spfs-cli-main migrate`↴](#spfs-cli-main-migrate)
* [`spfs-cli-main check`↴](#spfs-cli-main-check)
* [`spfs-cli-main read`↴](#spfs-cli-main-read)
* [`spfs-cli-main write`↴](#spfs-cli-main-write)
* [`spfs-cli-main docs`↴](#spfs-cli-main-docs)

## `spfs-cli-main`

SPK is a Package Manager for high-velocity software environments, built on SPFS. SPFS is a system for filesystem isolation, capture, and distribution.

**Usage:** `spfs-cli-main [OPTIONS] <COMMAND>`

EXTERNAL SUBCOMMANDS:
    render       render the contents of an environment or layer
    monitor      watch a runtime and clean it up when complete

###### **Subcommands:**

* `version` — Print the version of spfs
* `init` — Create an empty filesystem repository
* `edit` — Make the current runtime editable
* `commit` — Commit the current runtime state or a directory to storage
* `config` — Output the current configuration of spfs
* `reset` — Reset changes, or rebuild the entire spfs directory
* `run` — Run a program in a configured spfs environment
* `tag` — Tag an object
* `untag` — Remove tag versions or entire tag streams
* `shell` — Enter a subshell in a configured spfs environment
* `runtime` — View and manage spfs runtime information
* `layers` — List all layers in an spfs repository
* `platforms` — List all platforms in an spfs repository
* `tags` — List all tags in an spfs repository
* `info` — Display information about the current environment, or specific items
* `pull` — Pull one or more objects to the local repository
* `push` — Push one or more objects to a remote repository
* `log` — Log the history of a given tag over time
* `search` — Search for available tags by substring
* `diff` — Compare two spfs file system states
* `ls-tags` — List tags by their path
* `ls` — List the contents of a committed directory
* `migrate` — Migrate the data from and older repository format to the latest one
* `check` — Check a repositories internal integrity
* `read` — Output the contents of a blob to stdout
* `write` — Store an arbitrary blob of data in spfs
* `docs` — Output the current configuration of spfs

###### **Options:**

* `-v`, `--verbose` — Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` — Additionally log output to the provided file
* `--timestamp` — Enables timestamp in logging (always enabled in file log)



## `spfs-cli-main version`

Print the version of spfs

**Usage:** `spfs-cli-main version`



## `spfs-cli-main init`

Create an empty filesystem repository

**Usage:** `spfs-cli-main init <COMMAND>`

###### **Subcommands:**

* `repo` — Initialize an empty filesystem repository



## `spfs-cli-main init repo`

Initialize an empty filesystem repository

Does nothing when run on an existing repository

**Usage:** `spfs-cli-main init repo <PATH>`

###### **Arguments:**

* `<PATH>` — The root of the new repository



## `spfs-cli-main edit`

Make the current runtime editable

**Usage:** `spfs-cli-main edit [OPTIONS]`

###### **Options:**

* `--off` — Disable edit mode instead
* `--keep-runtime` — Change a runtime into a durable runtime, will also make the runtime editable



## `spfs-cli-main commit`

Commit the current runtime state or a directory to storage

**Usage:** `spfs-cli-main commit [OPTIONS] [KIND]`

###### **Arguments:**

* `<KIND>` — The desired object type to create, skip this when giving --path or --ref

  Possible values: `layer`, `platform`


###### **Options:**

* `-r`, `--remote <REMOTE>` — Commit files directly into a remote repository

   The default is to commit to the local repository. This flag is only valid with the --path argument.
* `-t`, `--tag <TAGS>` — A human-readable tag for the generated object

   Can be provided more than once.
* `--path <PATH>` — Commit this directory instead of the current spfs changes
* `--ref <REFERENCE>` — Combine existing items into a platform, use a '+' to join multiple
* `--hash-while-committing` — Hash the files while committing, rather than before.

   This option can improve commit times when a large number of the files are both large, and don't already exist in the repository. It may degrade commit times when committing directly to a slow or remote repository. When given, all files are written to the repository even if the payload exists, rather than hashing the file first to determine if it needs to be transferred.
* `--max-concurrent-blobs <MAX_CONCURRENT_BLOBS>` — The total number of blobs that can be committed concurrently

  Default value: `1000`
* `--max-concurrent-branches <MAX_CONCURRENT_BRANCHES>` — The total number of branches that can be processed concurrently at each level of the rendered file tree.

   The number of active trees being processed can grow exponentially by this exponent for each additional level of depth in the rendered file tree. In general, this number should be kept low.

  Default value: `5`



## `spfs-cli-main config`

Output the current configuration of spfs

**Usage:** `spfs-cli-main config`



## `spfs-cli-main reset`

Reset changes, or rebuild the entire spfs directory

**Usage:** `spfs-cli-main reset [OPTIONS] [PATHS]...`

###### **Arguments:**

* `<PATHS>` — Glob patterns in the spfs dir of files to reset, defaults to everything

###### **Options:**

* `--sync` — Sync the latest information for each tag even if it already exists
* `--check` — Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` — Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` — The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` — The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` — Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-e`, `--edit` — Mount the resulting runtime in edit mode

   Default to true if REF is empty or not given
* `-r`, `--ref <REFERENCE>` — The tag or id to rebuild the runtime with.

   Uses current runtime stack if not given. Use '-' or an empty string to request an empty environment. Only valid if no paths are given



## `spfs-cli-main run`

Run a program in a configured spfs environment

**Usage:** `spfs-cli-main run [OPTIONS] <--rerun <RUNTIME_NAME>|REFERENCE> [-- <COMMAND>...]`

###### **Arguments:**

* `<REFERENCE>` — The tag or id of the desired runtime

   Use '-' to or an empty string to request an empty environment
* `<COMMAND>` — The command to run in the environment and its arguments

   In order to ensure that flags are passed as-is, '--' must be place before specifying the command and any flags that should be given to that command: e.g. `spfs run <args> -- command --flag-for-command`

###### **Options:**

* `--sync` — Sync the latest information for each tag even if it already exists
* `--check` — Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` — Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` — The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` — The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` — Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose` — Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` — Additionally log output to the provided file
* `--timestamp` — Enables timestamp in logging (always enabled in file log)
* `-e`, `--edit` — Mount the spfs filesystem in edit mode (default if REF is empty or not given)
* `--no-edit` — Mount the spfs filesystem in read-only mode (default if REF is non-empty)
* `--force` — Requires --rerun. Force reset the process fields of the runtime before it is run again
* `-k`, `--keep-runtime` — Use to keep the runtime around rather than deleting it when the process exits. This is best used with '--name NAME' to make rerunning the runtime easier at a later time
* `--runtime-name <RUNTIME_NAME>` — Provide a name for this runtime to make it easier to identify
* `--rerun <RUNTIME_NAME>` — Name of an existing durable runtime to reuse for this run
* `--annotation <KEY:VALUE>` — Adds annotation key-value string data to the new runtime.

   This allows external processes to store arbitrary data in the runtimes they create. This is most useful with durable runtimes. The data can be retrieved by running `spfs runtime info` or `spfs info` and using the `--get <KEY>` or `--get-all` options

   Annotation data is specified as key-value string pairs separated by either an equals sign or colon (--annotation name=value --annotation other:value). Multiple pairs of annotation data can also be specified at once in yaml or json format (--annotation '{name: value, other: value}').

   Annotation data can also be given in a json or yaml file, by using the `--annotation-file <FILE>` argument. If given, `--annotation` arguments will supersede anything given in annotation files.

   If the same key is used more than once, the last key-value pair will override the earlier values for the same key.
* `--annotation-file <ANNOTATION_FILE>` — Specify annotation key-value data from a json or yaml file (see --annotation)



## `spfs-cli-main tag`

Tag an object

**Usage:** `spfs-cli-main tag [OPTIONS] <TARGET_REF> <TAG>...`

###### **Arguments:**

* `<TARGET_REF>` — The reference or id of the item to tag
* `<TAG>` — The tag(s) to point to the the given target

###### **Options:**

* `-r`, `--remote <REMOTE>` — Create tags in a remote repository instead of the local one



## `spfs-cli-main untag`

Remove tag versions or entire tag streams

**Usage:** `spfs-cli-main untag [OPTIONS] <TAG>`

###### **Arguments:**

* `<TAG>` — The tag to remove

   Unless --all or --latest is provided, this must have an explicit version number (eg: path/name~0)

###### **Options:**

* `-r`, `--remote <REMOTE>` — Remove tags in a remote repository instead of the local one
* `--latest` — Only remove the latest version of this tag
* `-a`, `--all` — Remove all versions of this tag, deleting it completely



## `spfs-cli-main shell`

Enter a subshell in a configured spfs environment

**Usage:** `spfs-cli-main shell [OPTIONS] <--rerun <RUNTIME_NAME>|REF>`

###### **Arguments:**

* `<REF>` — The tag or id of the desired runtime

   Use '-' or an empty string to request an empty environment

###### **Options:**

* `--sync` — Sync the latest information for each tag even if it already exists
* `--check` — Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` — Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` — The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` — The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` — Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose` — Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` — Additionally log output to the provided file
* `--timestamp` — Enables timestamp in logging (always enabled in file log)
* `-e`, `--edit` — Mount the spfs filesystem in edit mode (true if REF is empty or not given)
* `--no-edit` — Mount the spfs filesystem in read-only mode (default if REF is non-empty)
* `--rerun <RUNTIME_NAME>` — Name of a previously run durable runtime to reuse for this run
* `--force` — Requires --rerun. Force reset the process fields of the runtime before it is run again
* `--runtime-name <RUNTIME_NAME>` — Provide a name for this runtime to make it easier to identify
* `-k`, `--keep-runtime` — Use to keep the runtime around rather than deleting it when the process exits. This is best used with '--name NAME' to make rerunning the runtime easier at a later time
* `--annotation <KEY:VALUE>` — Adds annotation key-value string data to the new runtime.

   This allows external processes to store arbitrary data in the runtimes they create. This is most useful with durable runtimes. The data can be retrieved by running `spfs runtime info` or `spfs info` and using the `--get <KEY>` or `--get-all` options

   Annotation data is specified as key-value string pairs separated by either an equals sign or colon (--annotation name=value --annotation other:value). Multiple pairs of annotation data can also be specified at once in yaml or json format (--annotation '{name: value, other: value}').

   Annotation data can also be given in a json or yaml file, by using the `--annotation-file <FILE>` argument. If given, `--annotation` arguments will supersede anything given in annotation files.

   If the same key is used more than once, the last key-value pair will override the earlier values for the same key.
* `--annotation-file <ANNOTATION_FILE>` — Specify annotation key-value data from a json or yaml file (see --annotation)



## `spfs-cli-main runtime`

View and manage spfs runtime information

**Usage:** `spfs-cli-main runtime <COMMAND>`

**Command Alias:** `rt`

###### **Subcommands:**

* `info` — Show the complete state of a runtime
* `list` — List runtime information from the repository
* `prune` — Find and remove runtimes from the repository based on a pruning strategy
* `remove` — Remove runtimes from the repository



## `spfs-cli-main runtime info`

Show the complete state of a runtime

**Usage:** `spfs-cli-main runtime info [OPTIONS] <NAME>`

###### **Arguments:**

* `<NAME>` — The name/id of the runtime to remove

###### **Options:**

* `-r`, `--remote <REMOTE>` — Load a runtime in a remote or alternate repository
* `--get <GET>` — Output the data value for the given annotation key(s) from the active runtime. Each value is printed on its own line without its key
* `--get-all` — Output all the annotation keys and values from the active runtime as a yaml dictionary



## `spfs-cli-main runtime list`

List runtime information from the repository

**Usage:** `spfs-cli-main runtime list [OPTIONS]`

**Command Alias:** `ls`

###### **Options:**

* `-r`, `--remote <REMOTE>` — List runtimes in a remote or alternate repository
* `-q`, `--quiet` — Only print the name of each runtime, no additional data



## `spfs-cli-main runtime prune`

Find and remove runtimes from the repository based on a pruning strategy

**Usage:** `spfs-cli-main runtime prune [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` — Prune a runtime in a remote or alternate repository
* `--ignore-user` — Remove the runtime even if it's owned by someone else
* `--ignore-host` — Remove the runtime even if it appears to be from a different host

   Implies --ignore-monitor
* `--ignore-monitor` — Do not try and terminate the monitor process, just remove runtime data
* `--from-before-boot` — Remove runtimes started before last reboot



## `spfs-cli-main runtime remove`

Remove runtimes from the repository

**Usage:** `spfs-cli-main runtime remove [OPTIONS] [NAME]...`

**Command Alias:** `rm`

###### **Arguments:**

* `<NAME>` — The name/id of the runtime to remove

###### **Options:**

* `-r`, `--remote <REMOTE>` — Remove a runtime in a remote or alternate repository
* `-f`, `--force` — Remove the runtime from the repository forcefully

   Even if the monitor cannot be stopped or killed the data will be removed from the repository.
* `--ignore-user` — Remove the runtime even if it's owned by someone else
* `--ignore-host` — Remove the runtime even if it appears to be from a different host

   Implies --ignore-monitor
* `--ignore-monitor` — Do not try and terminate the monitor process, just remove runtime data
* `--remove-durable` — Allow durable runtimes to be removed, normally they will not be removed



## `spfs-cli-main layers`

List all layers in an spfs repository

**Usage:** `spfs-cli-main layers [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` — Show layers from remote repository instead of the local one
* `--short` — Show the shortened form of each reported layer digest
* `--tags` — Also find and report any tags that point to each layer, implies --short



## `spfs-cli-main platforms`

List all platforms in an spfs repository

**Usage:** `spfs-cli-main platforms [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` — Show layers from remote repository instead of the local one
* `--short` — Show the shortened form of each reported layer digest
* `--tags` — Also find and report any tags that point to each platform, implies --short



## `spfs-cli-main tags`

List all tags in an spfs repository

**Usage:** `spfs-cli-main tags [OPTIONS]`

###### **Options:**

* `-r`, `--remote <REMOTE>` — Show layers from remote repository instead of the local one
* `--target` — Also show the target digest of each tag
* `--short` — Show the shortened form of each reported digest, implies --target



## `spfs-cli-main info`

Display information about the current environment, or specific items

**Usage:** `spfs-cli-main info [OPTIONS] [REF]...`

###### **Arguments:**

* `<REF>` — Tag, id, or /spfs/file/path to show information about

###### **Options:**

* `-v`, `--verbose` — Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` — Additionally log output to the provided file
* `--timestamp` — Enables timestamp in logging (always enabled in file log)
* `--get <GET>` — Output the data value for the given annotation key(s) from the active runtime. Each value is printed on its own line without its key
* `--get-all` — Output all the annotation keys and values from the active runtime as a yaml dictionary
* `-H`, `--human-readable` — Lists file sizes in human readable format
* `-r`, `--remote <REMOTE>` — Operate on a remote repository instead of the local one

   This is really only helpful if you are providing a specific ref to look up.
* `--tags` — Also find and report any tags that point to any identified digest (implies '--short')
* `--short` — Use shortened digests in the output (nicer, but slower)
* `--follow` — Follow and show child objects, depth-first



## `spfs-cli-main pull`

Pull one or more objects to the local repository

**Usage:** `spfs-cli-main pull [OPTIONS] <REF>...`

###### **Arguments:**

* `<REF>` — The reference(s) to pull/localize

   These can be individual tags or digests, or they may also be a collection of items joined by a '+'

###### **Options:**

* `--sync` — Sync the latest information for each tag even if it already exists
* `--check` — Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` — Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` — The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` — The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` — Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose`
* `-r`, `--remote <REMOTE>` — The name or address of the remote server to pull from

   Defaults to searching all configured remotes



## `spfs-cli-main push`

Push one or more objects to a remote repository

**Usage:** `spfs-cli-main push [OPTIONS] <REF>...`

###### **Arguments:**

* `<REF>` — The reference(s) to push

   These can be individual tags or digests, or they may also be a collection of items joined by a '+'

###### **Options:**

* `--sync` — Sync the latest information for each tag even if it already exists
* `--check` — Traverse and check the entire graph, filling in any missing data

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted or lost, this option may help recover it.
* `--resync` — Forcefully sync all associated graph data even if it already exists

   When a repository is in good health, this should not be necessary, but if some subset of the data has been deleted, lost, or corrupted this option may help recover it.
* `--max-concurrent-manifests <MAX_CONCURRENT_MANIFESTS>` — The total number of manifests that can be synced concurrently

  Default value: `100`
* `--max-concurrent-payloads <MAX_CONCURRENT_PAYLOADS>` — The total number of file payloads that can be synced concurrently

  Default value: `100`
* `--progress <PROGRESS>` — Options for showing progress

  Possible values:
  - `bars`:
    Show progress bars (default)
  - `none`:
    Do not show any progress

* `-v`, `--verbose`
* `-r`, `--remote <REMOTE>` — The name or address of the remote server to push to

  Default value: `origin`



## `spfs-cli-main log`

Log the history of a given tag over time

**Usage:** `spfs-cli-main log [OPTIONS] <TAG>`

###### **Arguments:**

* `<TAG>` — The tag to show history of

###### **Options:**

* `-r`, `--remote <REMOTE>` — Load the tag from remote repository instead of the local one



## `spfs-cli-main search`

Search for available tags by substring

**Usage:** `spfs-cli-main search <TERM>`

###### **Arguments:**

* `<TERM>` — The search term/substring to look for



## `spfs-cli-main diff`

Compare two spfs file system states

**Usage:** `spfs-cli-main diff [FROM] [TO]`

###### **Arguments:**

* `<FROM>` — The tag or id to use as the base of the computed diff, defaults to the current runtime
* `<TO>` — The tag or id to diff the base against, defaults to the contents of the spfs filesystem



## `spfs-cli-main ls-tags`

List tags by their path

**Usage:** `spfs-cli-main ls-tags [OPTIONS] [PATH]`

**Command Alias:** `list-tags`

###### **Arguments:**

* `<PATH>` — The tag path to list under

  Default value: `/`

###### **Options:**

* `-r`, `--remote <REMOTE>` — List tags from a remote repository instead of the local one
* `--recursive` — Walk the tag tree recursively listing all tags under the specified dir



## `spfs-cli-main ls`

List the contents of a committed directory

**Usage:** `spfs-cli-main ls [OPTIONS] <REF> [PATH]`

**Command Aliases:** `list-dir`, `list`

###### **Arguments:**

* `<REF>` — The tag or digest of the file tree to read from
* `<PATH>` — The subdirectory to list

  Default value: `/spfs`

###### **Options:**

* `-r`, `--remote <REMOTE>` — List files on a remote repository instead of the local one
* `-R`, `--recursive` — Recursively list all files and directories
* `-l` — Long listing format
* `-H`, `--human-readable` — Lists file sizes in human readable format



## `spfs-cli-main migrate`

Migrate the data from and older repository format to the latest one

**Usage:** `spfs-cli-main migrate [OPTIONS] <PATH>`

###### **Arguments:**

* `<PATH>` — The path to the filesystem repository to migrate

###### **Options:**

* `--upgrade` — Replace old data with migrated data one complete



## `spfs-cli-main check`

Check a repositories internal integrity

**Usage:** `spfs-cli-main check [OPTIONS] [REF]...`

###### **Arguments:**

* `<REF>` — Objects to recursively check, defaults to everything

###### **Options:**

* `-r`, `--remote <REMOTE>` — Trigger the check operation on a remote repository instead of the local one
* `--max-tag-stream-concurrency <MAX_TAG_STREAM_CONCURRENCY>` — The maximum number of tag streams that can be read and processed at once

  Default value: `1000`
* `--max-object-concurrency <MAX_OBJECT_CONCURRENCY>` — The maximum number of objects that can be validated at once

  Default value: `5000`
* `--pull <PULL>` — Attempt to fix problems by pulling from another repository. Defaults to "origin"



## `spfs-cli-main read`

Output the contents of a blob to stdout

**Usage:** `spfs-cli-main read [OPTIONS] <REF> [PATH]`

**Command Aliases:** `read-file`, `cat`, `cat-file`

###### **Arguments:**

* `<REF>` — The tag or digest of the blob/payload to output
* `<PATH>` — If the given ref is not a blob, read the blob found at this path

###### **Options:**

* `-r`, `--remote <REMOTE>` — Read from a remote repository instead of the local one



## `spfs-cli-main write`

Store an arbitrary blob of data in spfs

**Usage:** `spfs-cli-main write [OPTIONS]`

**Command Alias:** `write-file`

###### **Options:**

* `-t`, `--tag <TAGS>` — A human-readable tag for the generated object

   Can be provided more than once.
* `-r`, `--remote <REMOTE>` — Write to a remote repository instead of the local one
* `-f`, `--file <FILE>` — Store the contents of this file instead of reading from stdin



## `spfs-cli-main docs`

Output the current configuration of spfs

**Usage:** `spfs-cli-main docs`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>
