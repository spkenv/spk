#!/bin/bash

# Copyright (c) 2022 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

# some simple tests to ensure that runtimes are properly cleaned up

assert_runtime_count() {
    count=$(spfs runtime list -q | wc -l)
    test $count -eq $1
}

# there's a runtime inside but not once exited
inner_count=$(spfs run - -- spfs runtime list -q | wc -l)
test $inner_count -eq 1
assert_runtime_count 0

# many runtimes at once
spfs run - -- sleep 4 &
spfs run - -- sleep 4 &
spfs run - -- sleep 4 &
spfs run - -- sleep 4 &
sleep 2
assert_runtime_count 4
wait
sleep 2
assert_runtime_count 0

# many runtimes launched recursively
spfs run - -- spfs run - -- spfs run - -- spfs run - -- sleep 4 &
sleep 2
assert_runtime_count 4
wait
sleep 2
assert_runtime_count 0

# fast runtime doesn't linger
spfs run - :
sleep 2
assert_runtime_count 0
