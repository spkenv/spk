---
title: spfs
chapter: False
---

SpFS is a file system isolation, capture and distribution system for software runtime management. In many ways, it's like docker-lite, providing some of the benefits of a container runtime while not creating too much isolation from the host environment.

### Key Concepts

#### File System

SpFS manages all files under the `/spfs` directory on your system. It has the ability to capture the exact state of this folder and reproduce it on any other spfs-enabled system.

#### Isolation

The contents of `/spfs` are managed _per-process-tree_. This means that one program running through spfs might see and entirely different set of files to another.

### Getting Started

```bash
# enter an empty spfs environment
spfs shell

# put some files into the spfs area
echo "hello, world" > /spfs/message.txt

ls /spfs
# message.txt
```

In a separate shell, we can see that the files are not visible.
```bash
ls /spfs
# <nothing>
```
{{% notice warning %}}
**Any files that you create, or changes that you make under /spfs are lost when the shell or program exists**. Make sure that you commit any changes that you want to keep or reuse (see below)
{{% /notice %}}

### Storage & Persistence

#### Persistence

When running in spfs, all file changes are stored in-memory by the underlying file system. This means that when the shell or program exists, all changes are lost. This also means that you can modify and test changes within an spfs runtime without affecting any other processes.

#### Tags, Layers, and Platforms

Under the hood, SpFS uses a layering system to build up the set of files that you see under the `/spfs` directory. Layers have a unique id which is derived from the files that it contains, but layers can also be tagged with a human-readable name to help keep track of it more easily. Platforms are just a stack of layers, and can also be tagged in the same way.

### Saving Your First Layer

The spfs _commit_ process is used to capture any active file changes and save them for use later.

```bash
# enter an empty spfs environment
spfs shell

# put some files into the spfs area
echo "hello, world" > /spfs/message.txt

# we can see our file in the set of active changes
spfs info

# Active Changes:
#  + /message.txt

# ask spfs to save the active changes into a new layer
# also give it a human-readable tag for easy reference
spfs commit layer --tag my-layer

# leave the spfs runtime
exit
```

```bash
# from a normal/new shell, the spfs area is empty
ls /spfs

# if we ask spfs to run the same command but with
# our tagged layer, we get a different result
spfs run my-layer -- ls /spfs
# message.txt

# using the tag name or layer id works for the shell
# command as well
spfs shell my-layer
ls /spfs
# message.txt
```

### Building a Platform

As mentioned above, a platform is simply a stack of layers. During the _commit_ step, we can optionally commit the entire stack of runtime layers instead of just the changeset. Using spenv with a platform tag is most common, because often a layer only represents a set of changes and the files onto which the changes were made are also important to the runtime environment.

```bash
# enter edit mode so that we can make changes on top of 'my-layer'
spfs shell my-layer --edit

echo "hello, platform!" > /spfs/platform-message.txt

# this will first create a layer from the active changes
# and then create and tag a platform that contains two layers:
#  -> <new changes>
#  -> my-layer
spfs commit platform --tag my-platform

# we can see that the stack is maintained in the platform
spfs info my-platform

# platform:
#  refs: XY64NZFA -> my-platform
#  stack:
#   - I22PAVLF
#   - FCQ6LOSW -> my-message
```

### Edit Mode

By default, when you run a command or enter into a shell with an existing spfs id or tag, the entire `/spfs` filesytem will be read-only. This means that, regardless of the file permissions, you won't be able to add files, remove files, or modify files in any way. Any active runtime can be made editable using the `spfs edit` command, or by bassing the `--edit` flag to the `spfs run` and `spfs shell` commands.

In edit mode, the spfs system stores changes that you make in a new area, layered on top of the existing files. This means that you are never actually modifying any files previously committed to spfs. There is not way to change a committed layer or platform, only update the tag to point to a newly committed set of files that are different (just like a git branch).

{{% notice info %}}
In edit mode, all changes are stored in memory and are lost when you exit the runtime. The only way to save these changes are to commit them as a layer or platform before exiting.
{{% /notice %}}

### Sharing References

The spfs _reference_ is any string that identifies either a layer or a platform (aka a tag or an id). Until now, we've been building and capturing our platforms and layers in the local storage. This means that the files, ids, and tags are only available to us. SpFS also allows us to share these items with others through the idea of remote storage.

#### Remote Storage

To SpFS a remote storage is simply any storage location that is not the default local storage. Each storage location has a name, with **origin** being the default remote storage (just like git). The set of available remote storages is defined in the spfs [config file](../configuration).

#### Pushing and Pulling References

When you ask spfs to run a shell or command with a given tag or id, it will first look for the requested reference in your local storage. If it doesn't find it there it will look for it in the configured remote storages next. If the reference is found in a remote storage, spfs will automatically _pull_ all of the necessary data into your local storage and then run the command. This process can also be triggered manually using the `spfs pull` command.

In order to share your layers and/platforms with others, you simply need to _push_ it into a remote storage that is configured on both ends.

```bash
# push our custom platform to the default remote storage
spfs push my-platform
```

SpFS will automatically push all of the required layers for our platform, but it won't include the tags that we have for the layers by default. We can push those separately, if desired.

```bash
spfs push my-layer
```

{{% notice tip %}}
Notice that spfs is very efficient with it's storage, and knows instantly that the layer already exists in the remote storage, so only creates the tag. (see [storage](../storage) for details)
{{% /notice %}}

### Further Reading

- The [Advanced Usage](./usage) documentation covers most of the next-level concepts that should be explored once the basics are understood.
