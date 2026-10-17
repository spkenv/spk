#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

# Bootstrap rootless Podman inside a disposable, privileged CI container.
# Podman and its subordinate-ID helpers must already be installed.
set -euo pipefail

if test "$(id -u)" != 0; then
    echo "Run this bootstrap as root inside a disposable CI container." >&2
    exit 1
fi

binary_dir="$(realpath "${1:?Pass the directory containing the built binaries}")"
tests_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Some container images omit the helpers' packaged file capabilities.
chmod u+s /usr/bin/newuidmap /usr/bin/newgidmap
useradd --create-home spfs-rootless
grep '^spfs-rootless:' /etc/subuid
grep '^spfs-rootless:' /etc/subgid
runtime_dir=/home/spfs-rootless/runtime
config_dir=/home/spfs-rootless/.config/containers
install -d -m 700 -o spfs-rootless -g spfs-rootless \
    "$runtime_dir" /home/spfs-rootless/.config "$config_dir"
# CI containers have no user systemd session. Avoid overlay-on-overlay storage
# for the nested container engine; /spfs itself still uses native overlayfs.
cat > "$config_dir/containers.conf" << EOF
[engine]
cgroup_manager = "cgroupfs"
events_logger = "file"
EOF
cat > "$config_dir/storage.conf" << EOF
[storage]
driver = "vfs"
EOF

exec runuser -u spfs-rootless -- env XDG_RUNTIME_DIR="$runtime_dir" \
    bash "$tests_dir/run_rootless_tests.sh" "$binary_dir"
