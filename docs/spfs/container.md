---
title: Running in a Container
summary: How to run spfs inside a Docker/Podman container, including the kernel module requirements for overlayfs.
---

# Running in a Container

spfs can run inside a container, but because it mounts an `overlayfs` filesystem at
`/spfs` it needs a few things from the container and the host that are not part of a
typical minimal base image. This guide covers both **Docker** and **Podman**.

The image build is identical for both runtimes; only the `run` invocation differs.
This guide uses Rocky Linux 9 as the example base, but the same requirements apply to
any distribution.

## Requirements

### 1. Elevated privileges

Mounting `overlayfs` requires `CAP_SYS_ADMIN`. The simplest way to grant what spfs
needs is to run the container as privileged:

```bash
docker run --privileged ...        # Docker
sudo podman run --privileged ...   # Podman (rootful)
```

Without it, spfs will fail the mount with an error such as `mount: /spfs: permission
denied` followed by `Failed to mount overlayfs`.

> [!CAUTION]
> **Rootless Podman is not supported for the `/spfs` mount.** A rootless container
> runs in a user namespace that disallows the `overlayfs` mount even with
> `--privileged`, so spfs fails with `mount: /spfs: permission denied`. Use **rootful
> Podman** (`sudo podman run ...`) or **Docker** (whose daemon already runs as root).
> The kernel-module requirements below still apply identically in all cases.

### 2. `kmod` and the kernel module tree (avoiding the overlayfs warning)

On startup, spfs detects which `overlayfs` mount options the running kernel supports
by running `/sbin/modinfo overlay`. If that command cannot run, spfs falls back to a
conservative set of mount options and logs:

```text
 WARN Failed to detect supported overlayfs params: ...
 WARN  > Falling back to the most conservative set, which is undesirable
 WARN  > To suppress this warning, set SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING=1
```

A minimal container image fails this check for two independent reasons, and **both**
must be addressed:

1. **`/sbin/modinfo` is missing.** It is provided by the `kmod` package, which is not
   present in minimal/container base images. Install it in your image. (The official
   spk/spfs RPMs declare `kmod` as a dependency, so this is only needed when building
   your own image from a base that lacks it.)

2. **The module metadata for the *running* kernel is missing.** `modinfo overlay`
   reads `/lib/modules/$(uname -r)/`, and a container shares the **host's** kernel.
   That directory is empty in the image, so the lookup fails. The reliable,
   kernel-version-agnostic fix is to bind-mount the host's modules read-only at
   runtime (the flag is the same for both Docker and Podman):

   ```bash
   docker run      -v /lib/modules:/lib/modules:ro ...
   sudo podman run -v /lib/modules:/lib/modules:ro ...
   ```

   Baking a fixed `kernel-modules-core` package into the image is fragile because it
   must exactly match whatever kernel the host is running.

> [!TIP]
> Setting `SPFS_SUPPRESS_OVERLAYFS_PARAMS_WARNING=1` only silences the message; spfs
> still uses the conservative mount options. Installing `kmod` and exposing
> `/lib/modules` lets spfs detect and use the full set of supported options, which is
> the recommended configuration.

## Example

### Building the image (Docker and Podman)

The same `Dockerfile`/`Containerfile` works for both runtimes. (Assuming you install
spfs from an RPM or build it in; see [Installation]({{< ref "../admin/install" >}}).)

```dockerfile
FROM rockylinux/rockylinux:9

# kmod provides /sbin/modinfo; fuse3 and rsync are spfs runtime deps.
# (Installing the spfs/spk RPM pulls these in automatically.)
RUN dnf install -y kmod fuse3 rsync

# ... install spfs here (RPM or copied-in binaries) ...
```

Build it with either tool:

```bash
docker build -t your-spfs-image .
podman build -t your-spfs-image .
```

### Running with Docker

```bash
docker run --rm --privileged \
    -v /lib/modules:/lib/modules:ro \
    your-spfs-image \
    spfs run - -- echo "hello from inside spfs"
```

### Running with Podman (rootful)

Run as root (or via `sudo`) so the `overlayfs` mount is permitted; rootless Podman
cannot mount `/spfs` (see the caution above):

```bash
sudo podman run --rm --privileged \
    -v /lib/modules:/lib/modules:ro \
    your-spfs-image \
    spfs run - -- echo "hello from inside spfs"
```

In both cases, with `kmod` installed and `/lib/modules` mounted, `modinfo overlay`
succeeds, the warning is gone, and spfs uses the full set of overlayfs options
supported by the host kernel.

## Verifying

Inside the running container you can confirm the prerequisites independently of spfs:

```bash
# should print the path, not "No such file or directory"
ls /sbin/modinfo

# should match the host kernel and contain that version's modules
uname -r
ls /lib/modules

# should exit 0 and print the module's parameters
modinfo overlay
```

If `modinfo overlay` exits 0, spfs will start without the overlayfs params warning.
