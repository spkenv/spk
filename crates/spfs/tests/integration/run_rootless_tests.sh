#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

# Exercise installed binaries as root and a non-root user in rootless Podman.
# Build the binaries first with `make debug-spfs`.
set -euo pipefail

source_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
binary_dir="$(realpath "${1:-$source_root/target/debug}")"
tests_dir="$source_root/crates/spfs/tests/integration"
image="localhost/spfs-rootless-tests:rocky9"

rootless="$(podman info --format '{{.Host.Security.Rootless}}')"
if test "$rootless" != true; then
    echo "This test requires rootless Podman; do not run it with sudo." >&2
    exit 1
fi
podman build -t "$image" \
    --build-arg "BASE_IMAGE=${SPFS_ROOTLESS_TEST_BASE_IMAGE:-docker.io/rockylinux/rockylinux:9.8}" \
    "$tests_dir/rootless"

for test_user in root tester; do
    podman run --rm --privileged \
        -v /lib/modules:/lib/modules:ro \
        -v "$source_root:/source:ro" \
        -v "$binary_dir:/binaries:ro" \
        "$image" bash -euc '
            for name in spfs spfs-clean spfs-enter spfs-fuse spfs-join spfs-monitor spfs-render; do
                install -m 755 "/binaries/$name" "/usr/local/bin/$name"
            done
            make -f /source/Makefile.linux setcap
            getcap /usr/local/bin/spfs-*
            exec runuser -u "$1" -- bash \
                /source/crates/spfs/tests/integration/rootless/test_overlayfs.sh
        ' bash "$test_user"
done
