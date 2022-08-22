#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# some simple tests to ensure that runtimes are properly cleaned up

assert_runtime_count() {
    # give any runtimes created by other tests a chance to expire
    # this number relates to the 2.5s poll interval when not using
    # cnproc.
    sleep 3

    count=$(spfs runtime list -q | wc -l)
    test $count -eq $1
}

# there's a runtime inside but not once exited
assert_runtime_count 0
inner_count=$(spfs run - -- spfs runtime list -q | wc -l)
test $inner_count -eq 1
assert_runtime_count 0

# many runtimes at once
spfs run - -- sleep 4 &
spfs run - -- sleep 4 &
spfs run - -- sleep 4 &
spfs run - -- sleep 4 &
assert_runtime_count 4
wait
assert_runtime_count 0

# many runtimes launched recursively
spfs run - -- spfs run - -- spfs run - -- spfs run - -- sleep 4 &
assert_runtime_count 4
wait
assert_runtime_count 0

# fast runtime doesn't linger
spfs run - :
assert_runtime_count 0
