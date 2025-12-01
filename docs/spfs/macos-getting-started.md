---
title: Getting Started with SPFS on macOS
---

# Getting Started with SPFS on macOS

This guide will help you set up and run SPFS on macOS using macFUSE.

## Prerequisites

### 1. Install macFUSE

macFUSE is a kernel extension that enables FUSE (Filesystem in Userspace) on macOS.

```bash
brew install --cask macfuse
```

**Important for Apple Silicon Macs**: You'll need to enable kernel extensions in Recovery Mode:

1. Restart your Mac and hold down the **power button** until you see "Loading startup options"
2. Click **Options** → **Continue**
3. From the menu bar, choose **Utilities** → **Startup Security Utility**
4. Select your startup disk and click **Security Policy**
5. Enable **Reduced Security** and check **"Allow user management of kernel extensions"**
6. Restart your Mac

After restart, you may see a System Settings notification asking you to allow the macFUSE extension. Click **Allow** and enter your password.

### 2. Verify macFUSE Installation

```bash
# Check if the macFUSE kernel extension is loaded
kextstat | grep fuse

# You should see output similar to:
# com.github.osxfuse.filesystems.osxfuse
```

### 3. Create the SPFS Mount Point

```bash
sudo mkdir -p /spfs
sudo chown $(whoami) /spfs
```

## Building SPFS

From the root of the SPK repository:

### Option A: Using Make (Recommended)

The Makefile automatically detects macOS and builds the correct binaries:

```bash
# Build debug binaries
make build

# Or build release binaries
make release

# Install debug binaries and set up /spfs mount point
make install-debug

# Or install release binaries
make install
```

The Makefile handles:
- Building `spfs-cli-fuse-macos` (the macOS-specific FUSE service)
- Creating the `/spfs` mount point with proper ownership
- Installing binaries to `/usr/local/bin`

### Option B: Using Cargo Directly

```bash
# Build the macOS FUSE service binary
cargo build --release -p spfs-cli-fuse-macos

# Build the main spfs CLI
cargo build --release -p spfs-cli

# Optional: Add binaries to your PATH
export PATH="$PWD/target/release:$PATH"
```

## Quick Start

### Option A: Manual Service Management (Recommended for Testing)

This approach gives you full control and visibility into the service.

**Terminal 1 - Start the service:**
```bash
spfs-fuse-macos service /spfs
```

You should see:
```
INFO FUSE mount started mountpoint=/spfs
INFO Service started listen=127.0.0.1:37738 mountpoint=/spfs
```

Leave this terminal running. Press Ctrl+C to stop the service when done.

**Terminal 2 - Use SPFS:**
```bash
# Run a command in an SPFS environment
spfs run my-package/1.0.0 -- ls /spfs

# Start an interactive shell
spfs shell my-package/1.0.0
ls /spfs
exit

# Start an editable shell
spfs shell --edit my-package/1.0.0
echo "new content" > /spfs/test.txt
cat /spfs/test.txt
exit
```

**Stop the service:**
```bash
# From another terminal:
spfs-fuse-macos service --stop

# Or press Ctrl+C in Terminal 1
```

### Option B: Auto-Start on Demand (Coming Soon)

Future versions will support automatic service startup when you first run `spfs run` or `spfs shell`. This will eliminate the need to manually manage the service.

## Usage Examples

### Read-Only Runtime

```bash
# Ensure service is running
spfs-fuse-macos service /spfs &

# Run a command
spfs run gcc/11.0.0 -- gcc --version

# Interactive shell
spfs shell gcc/11.0.0
which gcc
gcc --version
exit
```

### Editable Runtime

```bash
# Start an editable shell
spfs shell --edit my-build-env/1.0.0

# Make changes
echo "#!/bin/bash" > /spfs/bin/my-script
chmod +x /spfs/bin/my-script
mkdir /spfs/newdir
rm /spfs/old-file  # Delete existing file

# Exit shell
exit

# Commit changes
spfs commit layer -m "Added my-script and newdir"
```

### Multiple Isolated Environments

SPFS on macOS uses process-based isolation, allowing multiple environments to coexist:

```bash
# Terminal 1
spfs shell package-a/1.0.0
ls /spfs  # Shows package-a contents

# Terminal 2 (while Terminal 1 is still open)
spfs shell package-b/1.0.0
ls /spfs  # Shows package-b contents (different!)
```

Each process tree sees its own isolated view of `/spfs`.

## Known Limitations

### Writing to Existing Files

Currently, you cannot directly write to an existing file from the repository. You must copy it first:

```bash
# Workaround for modifying existing files
cp /spfs/lib/config.json /tmp/config.json.tmp
rm /spfs/lib/config.json
cp /tmp/config.json.tmp /spfs/lib/config.json
# Now you can edit /spfs/lib/config.json
```

Future versions will implement automatic copy-on-write for existing files.

### Scratch Directory Cleanup

The scratch directory (`/tmp/spfs-scratch-{runtime}/`) is not automatically cleaned up if a runtime crashes. You can manually remove orphaned directories:

```bash
ls /tmp/spfs-scratch-*
rm -rf /tmp/spfs-scratch-old-runtime-name
```

## Troubleshooting

### "Service is not running"

**Problem**: `spfs run` or `spfs shell` fails with "Service is not running"

**Solution**:
```bash
# Check if service is running
pgrep -f spfs-fuse-macos

# Start the service
spfs-fuse-macos service /spfs
```

### "Operation not permitted" on /spfs

**Problem**: Cannot access `/spfs` even with service running

**Solution**:
1. Verify macFUSE is loaded: `kextstat | grep fuse`
2. Check mount point permissions: `ls -ld /spfs`
3. Ensure `/spfs` is not already mounted: `mount | grep spfs`
4. If mounted incorrectly, unmount: `umount /spfs` or `diskutil unmount /spfs`
5. Restart the service

### "Failed to mount FUSE"

**Problem**: Service fails to start with mount errors

**Solution**:
1. Check if `/spfs` already has a mount:
   ```bash
   mount | grep spfs
   ```
2. Unmount if needed:
   ```bash
   umount /spfs
   # Or use diskutil if umount fails
   diskutil unmount /spfs
   ```
3. Verify no other FUSE processes are using `/spfs`:
   ```bash
   lsof /spfs
   ```
4. Restart the service

### Empty /spfs Directory

**Problem**: Process sees empty `/spfs` even though service is running

**Solution**:

This happens when the process is not a descendant of a registered runtime. Ensure:
1. You started the shell/command with `spfs run` or `spfs shell`
2. You're not running from a different terminal session that wasn't started by SPFS
3. The service is actually running: `pgrep -f spfs-fuse-macos`

### macFUSE Not Loading on Apple Silicon

**Problem**: Kernel extension won't load after installation

**Solution**:
1. Go to **System Settings** → **Privacy & Security**
2. Look for a message about blocked system extension
3. Click **Allow** next to the macFUSE/OSXFUSE extension
4. Restart your Mac
5. If still not working, boot into Recovery Mode and adjust security settings (see Prerequisites above)

## Advanced Configuration

### Custom Service Port

By default, the service listens on `127.0.0.1:37738`. To use a different port:

```bash
# Start service on custom port
spfs-fuse-macos service --listen 127.0.0.1:9999 /spfs

# Tell SPFS CLI to use custom port
export SPFS_MACFUSE_LISTEN_ADDRESS=127.0.0.1:9999
spfs shell my-package/1.0.0
```

### Running as a Background Service

To run the service in the background:

```bash
# Start in background
nohup spfs-fuse-macos service /spfs > /tmp/spfs-fuse.log 2>&1 &

# Check logs
tail -f /tmp/spfs-fuse.log

# Stop service
spfs-fuse-macos service --stop
```

### Logging

The service logs to syslog by default. View logs:

```bash
# View recent logs
log show --predicate 'process == "spfs-fuse-macos"' --last 1h

# Follow live logs
log stream --predicate 'process == "spfs-fuse-macos"'
```

## Architecture

For a deep dive into how SPFS works on macOS, see:
- [macOS FUSE Architecture](develop/macos-fuse-architecture.md)

Key differences from Linux:
- **No mount namespaces**: Uses PID-based routing instead
- **No OverlayFS**: Uses scratch directory for copy-on-write
- **Single FUSE mount**: All processes share `/spfs`, isolated by process ancestry

## Getting Help

If you encounter issues:
1. Check the troubleshooting section above
2. Review logs: `log show --predicate 'process == "spfs-fuse-macos"' --last 1h`
3. File an issue: https://github.com/spkenv/spk/issues

## Next Steps

- Read about [SPFS concepts and usage](usage.md)
- Learn about [creating packages](../use/create/packages.md)
- Explore [shell startup customization](startup.md)
