#!/bin/bash

# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

# test that spfs can be committed / remounted even if files are still open

cd /spfs

cat <<'EOF' | spfs run - -- bash -ex
cd ../spfs
echo "hello" > /spfs/foo.txt
exec 3< /spfs/foo.txt # open the file

spfs commit layer -t spfs-test/not-busy
test "$(cat <&3)" == "hello"
test "$(cat /spfs/foo.txt)" == "hello"
EOF
