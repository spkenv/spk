#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# when using fuse, the local repo should not get a renders directory created for
# the current user

temp_repo=$(mktemp -d)

cleanup() {
    rm -rf "$temp_repo"
}

trap cleanup EXIT

export SPFS_STORAGE_ROOT="$temp_repo"

# create some content that would need to be rendered
SPFS_FILESYSTEM_BACKEND=OverlayFsWithFuse spfs run - -- bash -c "echo hello > /spfs/hello.txt && spfs commit layer -t some-content"

# something that will open the local repo, using fuse
SPFS_FILESYSTEM_BACKEND=OverlayFsWithFuse spfs run some-content -- true

# the renders directory should not have been created
if test -d "$temp_repo/renders"; then
    echo "renders directory was not supposed to be created on step 1"
    exit 1
fi

# something that will open the local repo, not using fuse
SPFS_FILESYSTEM_BACKEND=OverlayFsWithRenders spfs run some-content -- true

# the renders directory should have been created, to prove the behavior is
# different when not using fuse
if test ! -d "$temp_repo/renders"; then
    echo "renders directory was expected to be created on step 2"
    exit 1
fi

