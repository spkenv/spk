#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# test that the standard `du` command is functional in a fuse-based runtime

# create a tag we can run a ro fuse runtime with
cat <<'EOF' | spfs run - -- bash -ex
dd if=/dev/zero of=/spfs/file_with_100_bytes1.dat bs=100 count=1 &> /dev/null
spfs commit layer -t spfs-test/fuse-du
EOF

# activate fuse usage
export SPFS_FILESYSTEM_BACKEND=FuseOnly

du_output=$(spfs run spfs-test/fuse-du -- bash -c "du -c /spfs/* | tail -n 1 | cut -f1")

# We wrote a file with some contents, so we expect a non-zero size out of du.
test "$du_output" != "0"