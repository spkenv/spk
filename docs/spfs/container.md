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

#### Rootless Podman

Rootless Podman also works (`podman run --privileged ...` without `sudo`), on
kernels that allow `overlayfs` mounts inside a user namespace (Linux 5.11 or newer).
A rootless container runs in its own user namespace, where the kernel does not allow
access to the `trusted.*` extended attributes that `overlayfs` normally uses. spfs
detects this and mounts `/spfs` with the `userxattr` option, which also forces
`redirect_dir=nofollow`, `index=off` and `metacopy=off` regardless of the configured
`filesystem.overlayfs_options`.

Older versions of spfs did not do this. On kernels with the reworked overlayfs
mount option parsing (including RHEL/Rocky 9.7 and newer), those versions fail with
`mount: /spfs: permission denied` and a kernel log message of `overlayfs: redirect_dir
requires permission to access trusted xattrs`. Earlier kernels (RHEL/Rocky 9.6 and
older) accepted the mount, but removing a directory that came from a lower layer could
fail with `Input/output error`.

The kernel-module requirements below apply in all cases.

#### Testing installed binaries

From a source checkout, build the debug binaries and run the rootless integration
matrix:

```bash
make debug-spfs
bash crates/spfs/tests/integration/run_rootless_tests.sh
```

The runner builds a Rocky Linux 9.8 image and installs the binaries with the same
file capabilities as `make install-spfs`. It tests both container root and a
non-root user through the mount-command and mount-syscall paths, with default
options and with conflicting redirect/index/metacopy settings. It checks the
actual mount options, read-only protection, remounting with `spfs edit`, and
directory removal followed by committing and reopening the resulting platform.
An optional first argument selects a different directory of built binaries.

Both CI systems run this matrix as well. GitHub runs it in the privileged Linux
build container using a dedicated rootless Podman user and a Debian Bookworm test
image matching the built binaries' glibc. The spdev
`spfs-rootless-integration-tests` component consumes the RPM artifacts and runs the
matrix in Rocky Linux 9.8. Both use the same CI bootstrap and test runner; a
rootful Podman invocation fails rather than silently changing what is tested.
For binaries built on another distribution, `SPFS_ROOTLESS_TEST_BASE_IMAGE`
selects a compatible Rocky or Debian base image.

These cases pass on a Rocky Linux 9.8 host running kernel
`5.14.0-687.34.1.el9_8.x86_64` with rootless Podman 5.8.2. The test configures a
local `origin` to avoid the independent
[missing-origin issue](https://github.com/spkenv/spk/issues/1412).
Removing and recreating a lower-layer directory is also checked in the live
runtime, but preserving its opaque-directory semantics after committing remains
a [known limitation](https://github.com/spkenv/spk/issues/1413).

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

### Running with Podman

The same invocation works rootful (with `sudo`) or rootless (without it; see
[Rootless Podman](#rootless-podman) above):

```bash
podman run --rm --privileged \
    -v /lib/modules:/lib/modules:ro \
    your-spfs-image \
    spfs run - -- echo "hello from inside spfs"
```

In all cases, with `kmod` installed and `/lib/modules` mounted, `modinfo overlay`
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
