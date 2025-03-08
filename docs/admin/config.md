---
title: Configuration
summary: Available configuration for spfs and spk
weight: 30
---

## Configuration Files

Both spfs and spk have their own configuration files. The spfs config file can be in json, yaml, toml or ini format. The files are discovered based on extension and loaded from the following locations:

For spfs: `/etc/spfs.toml`, which can be overridden by `~/.config/spfs/spfs.toml`
For spk: `/etc/spk.toml`, which can be overridden by `~/.config/spk/spk.toml`

### Environment Variables

All spfs and spk configuration values can be overridden in the environment. The name of the variable will be the upper-cased name of the config value, separated by underscores, and prefixed with either `SPFS_` or `SPK_`, eg: `SPFS_STORAGE_ROOT`. In cases where the name of the config value contains an underscore, two underscores can be used to disambiguate separators from names, eg: `SPFS_STORAGE__TAG_NAMESPACE`.

### SPFS Configuration

```toml
[storage]
# the file system path under which to store all locally created and
# committed spfs data. This should be a local disk with a decent
# amount of free storage space. Spfs supports the same storage
# being used by multiple users, but the directory must then be read
# and writable by anyone.
root = "~/.local/spfs"
# If true, when rendering payloads, allow hard links even if the payload
# is owned by a different user than the current user. Only applies to
# payloads readable by "other". This is often safe for shared read-only
# processes like render machines and artist workstations, but may cause
# permission issues for users that are authoring packages and layers.
allow_payload_sharing_between_users = false
# The tag namespace can be used to separate all spfs tags created in
# this repository from others, essentially segregating the data. This
# can be helpful to set per-user when shared local storage is used so
# that users don't see/edit/delete other's tags
# tag_namespace = "namespace"

# 'origin' is the default remote that should be configured
# for push and pull operations, typically a shared server or
# NFS filesystem. Any number of additional remotes with different
# names can be configured and used with the --remote flag available
# on most commands
[remote.origin]
# Remotes can either be configured by url address or by
# their full broken down configurations, with examples below
# Unless noted all of the extended parameters in the examples below
# can be also added as url parameters in an address.
address = "file:/tmp/spfs-origin"

[remote.filesystem-example]
scheme = "fs"
path = "/path/to/repository"
# create the repository if it does not already exist
create = false
# do not fail at startup if the repository does not exist,
# but only the first time it is used. This should be set
# when create=false but the path might not reliably be
# available and the
lazy = false
# pin the view of the repository to a point in time in the past.
# This only affects the state and availability of tag data
# (and spk packages) rather than object and file data.
# Tags cannot be modified or added to a pinned repository
when = "2020-06-15"
# optional tag namespace under which to store and read all tags
# see storage.tag_namespace for details
# tag_namespace = "namespace"

# the spfs server uses grpc as its communication protocol
[remote.grpc-example]
scheme = "grpc" # or "http2"
address = "my-domain.com:port"
# if true, don't actually attempt to connect until first use
lazy = false
# The global timeout for all requests made in this client
#
# Default is no timeout
timeout_ms = 100
# Maximum message size that the client will accept from the server
#
# Default is 4 Mb
max_decode_message_size_bytes = 1024
# Maximum message size that the client will sent to the server
#
# Default is no limit
max_encode_message_size_bytes = 1024
# see above on pinned repositories
when = "2020-06-15"
# see above on tag namespaces
# tag_namespace = "namespace"

# currently tar repositories must be extracted into a temporary
# folder while in use, and will be saved back into a tarball when
# the program exists. The main purpose for this is to export and
# import spk package archives. Please contact us on github if you
# have a more complex use for tarball repositories and would like
# to discuss a more efficient implementation
[remote.tar-example]
scheme = "tar"
path = "/path/to/archive.tar"

# The proxy repository reads and writes primarily to a single
# repository, but will read from the secondary repositories
# in the case of data missing from the primary one.
[remote.proxy-example]
scheme = "proxy"
# each or the primary and secondary repositories
# can either be the name of another remote in this config
# or a bespoke url address
primary = "origin"
secondary = ["file:/fallback-repository", "tar-example"]


[user]
# The username used when authoring tags.
# defaults to the current username as reported by the system.
name = "unknown"
# The domain of the user is attached to the username in tags,
# eg: user@domain. Defaults to the current hostname.
domain = "my-company.com"

[filesystem]
# The default mount backend to be used for new runtimes.
#
# OverlayFsWithRenders (linux)
#   Renders each layer to a folder on disk, before mounting
#   the whole stack as lower directories in overlayfs. Edits
#   are stored in the overlayfs upper directory.
#
# OverlayFsWithFuse (linux)
#   Mounts a fuse filesystem as the lower directory to
#   overlayfs, using the overlayfs upper directory for edits
#
# FuseOnly (linux)
#   Mounts a fuse filesystem directly
#
# WinFsp (windows)
#   Leverages the win file system protocol system to present
#   dynamic file system entries to runtime processes
#
backend = "OverlayFsWithRenders"
# The "mount" command will be used to mount overlayfs layers when false.
# Direct system calls will be used when true. Defaults to false.
# This option may be removed in the future and behave as if set to "true".
use_mount_syscalls = false
# The named remotes that can be used by the runtime
# file systems to find object data (if possible)
#
# This option is typically only relevant for virtual file
# systems that can perform read-through lookups, such as FUSE.
secondary_repositories = ["origin"]

[fuse]
# the number of threads that the fuse filesystem process will create
# in order to operate. More threads may improve performance of the
# mounted filesystem under heavy load, but may also eat resources
# when numerous runtimes are created on the same host at the same time.
# defaults to the lesser of the number of CPUs or 8
worker_threads = 8
# the number of blocking threads used for IO operations in the
# fuse filesystem process. This is a maximum, but blocking threads
# are created and destroyed based on demand.
max_blocking_threads = 512
# Enable a heartbeat between spfs-monitor and spfs-fuse. If spfs-monitor
# stops sending a heartbeat, spfs-fuse will shut down.
enable_heartbeat = true
# How often to send a heartbeat, in seconds
heartbeat_interval_seconds = 60
# How long to allow not receiving a heartbeat before shutting down, in seconds
heartbeat_grace_period_seconds = 300

[monitor]
# the number of threads that the monitor process will create
# in order to operate. This process does very little work so
# there is unlikely to be a need for more.
worker_threads = 2
# the number of blocking threads used for IO operations in the
# runtime monitor process.
max_blocking_threads = 2

# Optional environment variable names to preserve the value when creating an
# spfs runtime.
[environment]
variable_names_to_preserve = ["TMPDIR", "LD_LIBRARY_PATH"]
```

### SPK Configuration

The spk builds on the spfs configuration and adds settings around packaging. Notably, the package repositories are not part of this configuration as spk directly uses the remote repositories configured for spfs.

```toml

# Global metadata labels can be injected whenever a package is built.
#
# This is a list of commands that will output valid json when run.
[[metadata.global]]
command = ["command", "args"]
[[metadata.global]]
command = ["command2", "args"]

[solver]
# If true, the solver will run impossible request checks on the initial requests
check_impossible_initial = false
# If true, the solver will run impossible request checks before
# using a package build to resolve a request
check_impossible_validation = false
# If true, the solver will run impossible request checks to
# use in the build keys for ordering builds during the solve
check_impossible_builds = false
# Increase the solver's verbosity every time this many seconds pass
#
# A solve has taken too long if it runs for more than this
# number of seconds and hasn't found a solution. Setting this
# above zero will increase the verbosity every that many seconds
# the solve runs. If this is zero, the solver's verbosity will
# not increase during a solve.
too_long_seconds = 0
# The maximum verbosity that automatic verbosity increases will
# stop at and not go above.
verbosity_increase_limit = 0
# Maximum number of seconds to let the solver run before halting the solve
#
# Maximum number of seconds to allow a solver to run before
# halting the solve. If this is zero, which is the default, the
# timeout is disabled and the solver will run to completion.
solve_timeout = 0
# Set the threshold of a longer than acceptable solves, in seconds.
long_solve_threshold = 0
# Set the limit for how many of the most frequent errors are
# displayed in solve stats reports
max_frequent_errors = 0
# Comma-separated list of option names to promote to the front of the
# build key order.
build_key_name_order = ""
# Comma-separated list of option names to promote to the front of the
# resolve order.
request_priority_order = ""

# SPK supports the reporting of operational metrics to a
# statsd-compatible server for aggregation.
[statsd]
# Host name of the statsd server
host = ""
# Port number of the statsd server
port = 0
# Prefix to add to all statsd metrics
prefix = ""
# Format to use for statsd metrics, one of:
#   statsd
#   statsd-exporter-librato
format = "statsd"

# SPK supports automated error capture via sentry
[sentry]
# Sentry DSN
dsn = ""
# Sentry environment name
environment = ""
# If set, read the name of the user that sentry will report from this
# environment variable.
#
# This is useful in CI if the CI system has a variable that contains
# the username of the person who triggered the build.
# username_override_var = ""

# SPK supports configuration of these command line defaults
[cli.ls]
# Use all current host's host options by default for filtering in ls
host_filtering = false

# SPK supports some customization of the distro host options.
[host_options.distro_rules.rocky]
# Set a default compat rule for this distro. For example, on Rocky Linux
# packages built on 9.3 would be usable on 9.4.
compat_rule = "x.ab"
```
