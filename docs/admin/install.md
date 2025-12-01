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

### macOS

SPFS on macOS uses macFUSE for filesystem operations. After cloning the repository:

```sh
# Install macFUSE and dependencies
brew install --cask macfuse
brew install cmake protobuf pkg-config openssl

# Install FlatBuffers compiler (v23.5.26)
FB_REL=https://github.com/google/flatbuffers/releases/
curl --proto '=https' --tlsv1.2 -sSfL ${FB_REL}/download/v23.5.26/Mac.flatc.binary.zip -o /tmp/flatc.zip
cd /tmp && unzip -o flatc.zip && sudo mv flatc /usr/local/bin/flatc && sudo chmod +x /usr/local/bin/flatc

# Build and install
make install      # both spfs and spk
make install-spfs # only spfs
```

The Makefile automatically detects macOS and builds the correct binaries (`spfs-cli-fuse-macos`). It also creates the `/spfs` mount point with proper ownership.

**Apple Silicon users** may need to enable kernel extensions in Recovery Mode. See the [macOS Getting Started Guide](../spfs/macos-getting-started.md) for details.

### Windows

Currently, only spfs is supported on windows and is still considered experimental. File systems can be mounted and viewed, but not modified. See above on building from source - windows builds will require WinFSP to be installed rather than fuse libraries.

<!-- TODO: include really basic make instructions as above for playing with this -->
