---
title: Runtime Semantics
---

# Runtime Semantics

Details on startup and runtime procedures.

SpFS leverages linux namespaces in order to provide a per-process view of the `/spfs` filesystem. To render the filesystem itself, `overlayfs` is used.

### OverlayFS and Rendering Layers

`overlayfs` is a filesystem that is built into the linux kernel. It allows multiple directories to be layered on top of each other and mounted as a single view. It also keeps a working layer at the very top to store all changes made to the filesystem, leaving all of the lower layers unchanged.

Since spfs layers are stored and identified by a hash of their contents, this immutability is a key feature to how the system works. In order to deduplicate and look up graph data quickly, spfs stores all file and object data on disk using their digest. `overlayfs`, however, requires that each layer be laid out in the filesystem as it would be viewed by the user. For this reason, there is an additional `ManifestViewer` trait that can be supported by repositories, which provides a local path to what is called a _rendered_ view of a manifest.

The current filesystem repository creates these renders by hard-linking objects into this tree. We cannot avoid using extra inodes for these renders but this way we do not bloat the disk usage.

### Runtime Structure

The spfs runtime is set up to support the desired workflows for building, committing and reusing filesystem layers.

In addition to all of the base filesystem layers, `overlayfs` requires a working directory in which to store any changes made to the filesystem. Because the `overlayfs` filesystem is run by root, it can create files in this directory which cannot be cleaned up by a normal user. For this reason, the working directory is placed into an in-memory `tmpfs` mount which will simply destroy anything in this working directory when it unmounts.

To keep the `/spfs` and `tmpfs` mount separated per-process, they are both setup in a new linux namespace during the spfs startup/initialization process. This process requires special privileges, and so are handled by a separate `spfs-enter` binary that is installed with these capabilities attached.

### Runtime Startup, Bootstrapping and Environments

To launch a new environment, spfs runs through a few distinct stages:

First, spfs takes the tags or digests given at the command line and resolves them into a set of filesystem layers to use. For each layer it ensures that it is available in the local repository and that it has been rendered for use with overlayfs. At this stage, spfs also determines which files, if any, exist in a lower layer but need to be removed/masked by an upper layer.

With this information, spfs then calls the `spfs-enter` command, providing all the layers, deleted files and other runtime details as command line arguments. This spfs-enter command sets up the namespace, mounts the filesystem and adds a mask for any deleted file. It then calls back into the `spfs init-runtime` command.

Finally, the init-runtime command determines which shell will be used to set up the environment and then calls through a startup script. This startup script is written for each supported shell and manages the sourcing of `startup.d` activation scripts before ultimately giving control to the user (for `spfs shell` sessions) or launching the desired subprocess.
