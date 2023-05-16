#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# no spfs-fuse processes should survive past runtime cleanup

assert_spfs_fuse_count() {
    count=$(ps -ef | grep spfs-fuse | grep -v grep | wc -l)
    test $count -eq $1
}

# create a tag we can run a ro fuse runtime with
cat <<'EOF' | spfs run - -- bash -ex
echo $SPFS_RUNTIME
echo "hello" > /spfs/foo.txt
spfs commit layer -t spfs-test/fuse-cleanup
EOF

# give this runtime a chance to cleanup
sleep 5

# baseline no spfs-fuse processes
assert_spfs_fuse_count 0

# activate fuse usage
export SPFS_FILESYSTEM_BACKEND=FuseOnly

# while a runtime exists there should be one spfs-fuse process
spfs run spfs-test/fuse-cleanup -- sleep 2 &
sleep 1
assert_spfs_fuse_count 1

# allow runtime to cleanup
sleep 10

# now expected to be back to 0
assert_spfs_fuse_count 0