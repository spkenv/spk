---
title: Advanced Usage
---

# Advanced Usage

Additional command line workflows for more advanced users.

## Tag Streams

When tags are created in spfs, they are added to what is known as a _tag stream_. Tag streams are simply a historical record of that tag over time, keeping track of each change, when it was made, and by whom. Previous versions of a tag can be referenced using a tilde, where the most recent version of a tag is version `~0`; the previous version is version `~1`; the version before that is version `~2` etc... This notation can be used everywhere a tag can be used. This means that `spfs run my-tag` is the same as `spfs run my-tag~0`. All the available versions of a tag can be viewed using the `spfs log <tag>` command, where you will see this notation used.

```bash
spfs shell

echo 1 > /spfs/message.txt
spfs commit layer --tag my-layer

spfs edit
echo 2 > /spfs/message.txt
spfs commit layer --tag my-layer

spfs edit
echo 3 > /spfs/message.txt
spfs commit layer --tag my-layer

spfs log my-layer
# 6E5CA5XL3L my-layer    rbottriell@wolf0254.spimageworks.com 2020-03-18 10:12
# 6E5CA5XL3L my-layer~1  rbottriell@wolf0254.spimageworks.com 2020-03-18 10:11
# XHHVG3NDGE my-layer~2  rbottriell@wolf0254.spimageworks.com 2020-03-18 10:11
```

### Reverting a Tag

Using a tag stream, we can revert to previous versions of a tag by simply re-tagging the older version as the latest one. To continue from the example above:

```bash
spfs tag my-layer~2 my-layer

spfs log my-layer
# XHHVG3NDGE my-layer    rbottriell@wolf0254.spimageworks.com 2020-03-18 10:16
# 6E5CA5XL3L my-layer~1  rbottriell@wolf0254.spimageworks.com 2020-03-18 10:12
# JJ3MEJOYQ2 my-layer~2  rbottriell@wolf0254.spimageworks.com 2020-03-18 10:11
# XHHVG3NDGE my-layer~3  rbottriell@wolf0254.spimageworks.com 2020-03-18 10:11
```

{{% notice tip %}}
If you want to see or update shared tags, remember to specify the remote repository for each command (eg: `spfs log my-layer -r origin`)
{{% /notice %}}

## Diff Tool

Any two spfs file system states can be compared using the `spfs diff` command. With no arguments, this command works much like the `git status` command, showing the current set of active changes that have not been committed (if you are in an spfs runtime).

##

It's easy enough to pull and mount an spfs file tree, but sometimes it's not ideal to have to localize or sync the entire thing just to get a little bit of information or check the contents of a key file. SpFS provides 2 commands which allow for easy introspection of committed data without the need to enter into the environment itself.

- `spfs ls` can be used to list directory contents of a stored file tree
- `spfs cat` can be used to output the contents of a file stored in spfs

```bash
spfs shell
mkdir -p /spfs/bin
echo "I am root" > /spfs/root.txt
touch /spfs/bin/command
spfs commit layer --tag simple-fs
# exit the spfs runtime
exit

spfs info simple-fs
# layer:
#  refs: YJGTUV2Y -> simple-fs
#  manifest: EDAWAZUS

spfs ls simple-fs
# bin
# root.txt

spfs ls simple-fs bin
# command

spfs cat simple-fs root.txt
# I am root
```

## Repository Cleaning

Over time, an spfs repository can get quite large, as it retains data from long ago that may not be used anymore as well as containing data for committed platforms, layers and blobs that are not referenced in any tag. The `spfs clean` command can be used to remove such data, as well as to find and remove old data for past tag history which is no longer desired. By default, the clean command will only find and print information about things that would be removed, and must be explicitly told to delete data.

{{% notice warning %}}
These commands can and will remove data, and should be used with great caution.
{{% /notice %}}

```bash
spfs clean --help
```

Objects are considered to be attached, and unremovable if they are reachable from any version of any tag in the repository. The `--prune` flag and related options can be used to get rid of older tag versions based on age or number of versions before cleaning the repository. This is a good way to try and disconnect additional objects, create more data that can be cleaned.

{{% notice tip %}}
The pruning process will always prefer keeping a tag version over removing it when multiple keep/prune conditions apply to it. Check the default values for each setting if you expected more tags than were shown.
{{% /notice %}}

## Temporary Filesystem Size

The spfs runtime uses a temporary, in-memory filesystem, which means that large sets of changes can run out of space because of RAM limitations. The size of this filesystem can be overridden using the `SPFS_FILESYSTEM_TMPFS_SIZE` variable (eg `SPFS_FILESYSTEM_TMPFS_SIZE=10G`). Note that specifying values close to or larger than the available memory on the system may cause deadlocks or system instability.


## Live Layers: external directories and files in a spfs runtime

Spfs supports adding external directories and files on top of an /spfs runtime. These are known as live layers in Spfs. They can be used to include things like local git repo checkouts of code directly inside /spfs to aid development, debugging, and allow normal git commands to operate insdie that part of /spfs.

A live layer is configued by a yaml file, called `layer.spfs.yaml` by default.

You can give `spfs run` the path to a live layer file, or the path to a directory that contains a 'layer.spfs.yaml' file, as one of the REFS on the command line that will make up the spfs runtime, e.g. `spfs run digest+digest+liverlayerfile+tag+digest`. Multiple files can be specified on the command line. `spfs run` will put a live layer into /spfs each for config file specified.

Example `layer.spfs.yaml` file in `/some/directory/somewhere/`:

```yaml
# layer.spfs.yaml
api: v0/layer
contents:
  - bind: docs/use
    dest: /spfs/docs
  - bind: tests/some.data
    dest: test_data/some.data
```

The `api:` field is required to indicate which version of live layer is in the file.

The `contents:` field is required and tells spfs what this live layer will add into /spfs. It is a list of items. Currently spfs supports bind mount items in live layers. Each bind mount consists of a source (`bind:` or `src:`) path and a destination (`dest:`) path. 

Each source path must be within the directory that the `layer.spfs.yaml` is in. For the example live layer above to be valid, its parent directory must contain these sub-directories and files (from its `bind:` fields):
- docs/use
- tests/some.data

Each destination path will be relative to /spfs. You can specify /spfs in a destination path or not, spfs will add it as needed. If a destination location doesn't exist under /spfs, spfs will create it (by making a new spfs layer at runtime that contains all the destinations).
