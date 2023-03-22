#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# test that a file with restrictive perms created by one user can be accessed
# by a second user.

SPFS=/usr/local/bin/spfs

content="hello world"
# this content produces a digest with...
digest_part1="VF"
digest_part2="EJATZPB5DZXD4BS5UUWMAYJMGS5UOBZUVB5QH3QXJJTIMSURDQ===="

filepath="/spfs/file1";
base_tag="test/restrictive-perms";

# we want these two users to share the same local repo
export SPFS_STORAGE_ROOT=/tmp/spfs-repos-local
rm -rf $SPFS_STORAGE_ROOT
mkdir -p $SPFS_STORAGE_ROOT
chmod 777 $SPFS_STORAGE_ROOT

# commit a layer with a file that is only readable by the current user

sudo -E -u user1 $SPFS run - -- bash -ex <<EOF
echo "$content" > "$filepath"
chmod 0600 "$filepath"
$SPFS commit layer -t "$base_tag"
EOF

# try to access that restricted file as a different user

sudo -E -u user2 bash -ex <<EOF
$SPFS run "$base_tag" -- cat "$filepath"
EOF

payload_inode=$(stat --format="%i" $SPFS_STORAGE_ROOT/payloads/$digest_part1/$digest_part2)

# we expect the proxy file for user1 to be a different inode (a copy) because
# the rendered file does not have the same perms as the payload
user1_proxy_inode=$(stat --format="%i" $SPFS_STORAGE_ROOT/renders/user1/proxy/$digest_part1/$digest_part2/33152)
test $payload_inode -ne $user1_proxy_inode

# we expect the proxy file for user2 to be a different inode (a copy)
user2_proxy_inode=$(stat --format="%i" $SPFS_STORAGE_ROOT/renders/user2/proxy/$digest_part1/$digest_part2/33152)
test $payload_inode -ne $user2_proxy_inode

# futhermore the two users should have different copies because the each need
# ownership of their respective copies.
test $user1_proxy_inode -ne $user2_proxy_inode