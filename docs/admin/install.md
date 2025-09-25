---
title: Installation
summary: Installation instructions for spfs and spk
weight: 10
---

### Linux (RHEL/Alma/Rocky)

RPM packages are published with each release, and can be downloaded from [GitHub](https://github.com/spkenv/spk/releases).

### Linux (From Source)

After cloning the repository, ensure that cargo (the rust build tool) and make are available. The project can then be built and installed on a local machine by running. You may also require other build dependencies, like `fuse3-devel` depending on the components of your system. A typical list of dependencies can be found in the [rpm spec file](https://github.com/spkenv/spk/blob/main/spk.spec).

```sh
make install      # both spfs and spk
make install-spfs # only spfs
```

### Windows

Currently, only spfs is supported on windows and is still considered experimental. File systems can be mounted and viewed, but not modified. See above on building from source - windows builds will require WinFSP to be installed rather than fuse libraries.

<!-- TODO: include really basic make instructions as above for playing with this -->
