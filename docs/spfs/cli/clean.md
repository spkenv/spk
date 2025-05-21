---
title: Clean
chapter: true
---

# Command-Line Help for `spfs-clean`

This document contains the help content for the `spfs-clean` command-line program.

**Command Overview:**

* [`spfs-clean`Γå┤](#spfs-clean)

## `spfs-clean`

Clean the repository storage of any untracked data

Untracked data is any data that is not tagged or is not attached to/used by a tagged object. This command also provides semantics for pruning a repository from older tag data to help detach additional data and reduce repository size.

**Usage:** `spfs-clean [OPTIONS]`

###### **Options:**

* `-v`, `--verbose` ΓÇö Make output more verbose, can be specified more than once
* `--log-file <LOG_FILE>` ΓÇö Additionally log output to the provided file
* `--timestamp` ΓÇö Enables timestamp in logging (always enabled in file log)
* `-r`, `--remote <REMOTE>` ΓÇö Trigger the clean operation on a remote repository
* `--remove-durable <RUNTIME>` ΓÇö Remove the durable upper path component of the named runtime. If given, this will be the only thing removed
* `--runtime-storage <RUNTIME_STORAGE>` ΓÇö The address of the storage being used for runtimes

   Defaults to the current configured local repository.
* `-y`, `--yes` ΓÇö Don't prompt/ask before cleaning the data
* `--dry-run` ΓÇö Don't delete anything, just print what would be deleted (assumes --yes)
* `--prune-repeated` ΓÇö Prune old tags that have the same target as a more recent version
* `--prune-repeated-keep <PRUNE_REPEATED_KEEP>` ΓÇö When pruning old tag that have the same target as a more recent version, keep this many of the repeated tags
* `--prune-if-older-than <PRUNE_IF_OLDER_THAN>` ΓÇö Prune tags older that the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s)
* `--keep-if-newer-than <KEEP_IF_NEWER_THAN>` ΓÇö Always keep data newer than the given age (eg: 1y, 8w, 10d, 3h, 4m, 8s)
* `--prune-if-more-than <PRUNE_IF_MORE_THAN>` ΓÇö Prune tags if there are more than this number in a stream
* `--keep-if-less-than <KEEP_IF_LESS_THAN>` ΓÇö Always keep at least this number of tags in a stream
* `--keep-proxies-with-no-links` ΓÇö Do not remove proxies for users that have no additional hard links.

   Proxies will still be removed if the object is unattached. This is enabled by default because it is generally considered safe and can be effective at reducing disk usage.
* `--max-tag-stream-concurrency <MAX_TAG_STREAM_CONCURRENCY>`

  Default value: `500`
* `--max-removal-concurrency <MAX_REMOVAL_CONCURRENCY>`

  Default value: `500`
* `--max-discover-concurrency <MAX_DISCOVER_CONCURRENCY>`

  Default value: `50`



<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

