#!/bin/bash

# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# test that a tags can be added and removed as expected

filename="/spfs/message.txt";
base_tag="test/tagging_base";

spfs untag $base_tag --all >& /dev/null || true;

spfs run - -- bash -c "echo hello1 > $filename && spfs commit layer -t $base_tag"
spfs run - -- bash -c "echo hello2 > $filename && spfs commit layer -t $base_tag"
spfs run - -- bash -c "echo hello3 > $filename && spfs commit layer -t $base_tag"

version_0=$(spfs info $base_tag~0 | grep manifest)
version_1=$(spfs info $base_tag~1 | grep manifest)
version_2=$(spfs info $base_tag~2 | grep manifest)

spfs untag $base_tag~1
test $(spfs log $base_tag | wc -l) -eq 2 # there should now only be two tag versions
test "$(spfs info $base_tag~1 | grep manifest)" == "$version_2" # should remove middle tag

spfs untag $base_tag --latest
test $(spfs log $base_tag | wc -l) -eq 1 # there should now only be one tag version
test "$(spfs info $base_tag | grep manifest)" == "$version_2" # should remove first tag

spfs untag --all $base_tag
set +o errexit
spfs info $base_tag >& /dev/null
test $? -eq 1 # there should be no tag versions now
