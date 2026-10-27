#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

# Check real mounts and committed whiteouts, not just successful commands.
set -euo pipefail

cat /etc/os-release
uname -r
id
cat /proc/self/uid_map
modinfo overlay > /dev/null
test "$(stat -c %u /usr/local/bin/spfs-enter)" = 0
getcap /usr/local/bin/spfs-enter | grep -q cap_sys_admin

for syscalls in false true; do
    for options in default conflicting; do
        echo "Testing user=$(id -un) syscalls=$syscalls options=$options"
        export SPFS_STORAGE_ROOT
        SPFS_STORAGE_ROOT="$(mktemp -d)"
        # A local origin avoids the independent missing-origin bug (#1412).
        export SPFS_REMOTE_ORIGIN_ADDRESS="file://$SPFS_STORAGE_ROOT"
        export SPFS_FILESYSTEM__USE_MOUNT_SYSCALLS="$syscalls"
        if test "$options" = conflicting; then
            export SPFS_FILESYSTEM__OVERLAYFS_OPTIONS__REDIRECT_DIR=on
            export SPFS_FILESYSTEM__OVERLAYFS_OPTIONS__BREAK_HARDLINKS=false
            export SPFS_FILESYSTEM__OVERLAYFS_OPTIONS__METADATA_COPY_UP=true
        else
            unset SPFS_FILESYSTEM__OVERLAYFS_OPTIONS__REDIRECT_DIR
            unset SPFS_FILESYSTEM__OVERLAYFS_OPTIONS__BREAK_HARDLINKS
            unset SPFS_FILESYSTEM__OVERLAYFS_OPTIONS__METADATA_COPY_UP
        fi
        spfs config | grep -q "\"use_mount_syscalls\": $syscalls"
        if test "$options" = conflicting; then
            spfs config | grep -q '"redirect_dir": "On"'
            spfs config | grep -q '"break_hardlinks": false'
            spfs config | grep -q '"metadata_copy_up": true'
        fi

        spfs -v run -e - -- bash -euc '
            options="$(findmnt -n -o OPTIONS /spfs)"
            echo "Editable mount: $options"
            [[ ",$options," = *,userxattr,* ]]
            [[ ",$options," != *,redirect_dir=on,* ]]
            [[ ",$options," != *,redirect_dir=follow,* ]]
            [[ ",$options," != *,index=on,* ]]
            [[ ",$options," != *,metacopy=on,* ]]
            mkdir -p /spfs/keep/remove
            echo original > /spfs/keep/remove/old
            spfs commit layer -t rootless/base
        '
        spfs run rootless/base -- bash -euc '
            test "$(cat /spfs/keep/remove/old)" = original
            findmnt -n -o OPTIONS /spfs | grep -q "^ro,"
            if touch /spfs/should-not-exist; then
                echo "Read-only runtime accepted a write" >&2
                exit 1
            fi
            spfs edit
            test "$(cat /spfs/keep/remove/old)" = original
            echo remounted > /spfs/remounted
            test "$(cat /spfs/remounted)" = remounted
        '
        spfs run -e rootless/base -- bash -euc '
            test "$(cat /spfs/keep/remove/old)" = original
            rm -r /spfs/keep/remove
            spfs commit platform -t rootless/removed
        '
        spfs run rootless/removed -- bash -euc '
            test -d /spfs/keep
            test ! -e /spfs/keep/remove
        '
        # Recreating an opaque directory and committing its contents has a
        # separate known limitation (#1413); verify the live view here.
        spfs run -e rootless/base -- bash -euc '
            rm -r /spfs/keep/remove
            mkdir /spfs/keep/remove
            test ! -e /spfs/keep/remove/old
            echo replacement > /spfs/keep/remove/new
            spfs commit layer -t rootless/recreated
        '
        echo "PASS user=$(id -un) syscalls=$syscalls options=$options"
    done
done
