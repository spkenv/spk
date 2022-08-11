---
title: Configuration
---

# Configuring SpFS

## Config File

The spfs config file is loaded from the following locations:
- /etc/spfs.conf
- ~/.config/spfs/spfs.conf

### Config Options

```ini
[storage]
# the file system path under which to
# store all locally created and committed
# spfs data. This should be a local disk
# with a decent amount of free storage space
root = "~/.local/spfs"

# 'origin' is the default remote that should be
# configured for push and pull operations, but
# any number of additional remotes with different
# names can be configured and used with the
# --remote flag available on most commands
[remote.<name>]
# address is a URL where the remote is accessible
# the 'file' protocol is a local file path, but
# other protocols may also be available
address = file:/path/to/remote
```
