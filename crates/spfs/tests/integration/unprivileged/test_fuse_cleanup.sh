#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# no spfs-fuse processes should survive past runtime cleanup

assert_spfs_fuse_count() {
    # Don't count any defunct processes; on github actions there is an issue
    # with the init process not reaping processes.
    count=$(ps -ef | grep spfs-fuse | grep -v grep | grep -v defunct | wc -l)
    test $count -eq $1
}

get_spfs_monitor_count() {
    # Don't count any defunct processes; on github actions there is an issue
    # with the init process not reaping processes.
    count=$(ps -ef | grep spfs-monitor | grep -v grep | grep -v defunct | wc -l)
    echo $count
}

wait_for_spfs_monitor_count() {
    set +x
    if test $(get_spfs_monitor_count) -ne $1; then
        echo waiting for monitors...
    fi
    until test $(get_spfs_monitor_count) -eq $1; do sleep 2; done
    sleep 2;
    set -x
}

# create a tag we can run a ro fuse runtime with
cat <<'EOF' | spfs run - -- bash -ex
echo $SPFS_RUNTIME
echo "hello" > /spfs/foo.txt
spfs commit layer -t spfs-test/fuse-cleanup
EOF

# give this runtime a chance to cleanup
wait_for_spfs_monitor_count 0

# baseline no spfs-fuse processes
assert_spfs_fuse_count 0

# activate fuse usage
export SPFS_FILESYSTEM_BACKEND=FuseOnly

# while a runtime exists there should be one spfs-fuse process
spfs run spfs-test/fuse-cleanup -- bash -c 'ls /spfs; sleep 2' &
wait_for_spfs_monitor_count 1
assert_spfs_fuse_count 1

# allow runtime to cleanup
wait
wait_for_spfs_monitor_count 0

# now expected to be back to 0
assert_spfs_fuse_count 0
