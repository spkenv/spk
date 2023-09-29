#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# some simple tests to ensure that runtimes are properly cleaned up

assert_runtime_count() {
    count=$(spfs runtime list -q | wc -l)
    test $count -eq $1
}

get_spfs_monitor_count() {
    # Don't count any defunct processes; on github actions there is an issue
    # with the init process not reaping processes.
    count=$(ps -ef | grep spfs-monitor | grep -v grep | grep -v defunct | wc -l)
    echo $count
}

wait_for_spfs_monitor_count() {
    expected_count=$1
    sleep_duration=${2:-2}

    set +x
    if test $(get_spfs_monitor_count) -ne $expected_count; then
        echo waiting for monitors...
    fi
    until test $(get_spfs_monitor_count) -eq $expected_count; do sleep $sleep_duration; done
    sleep $sleep_duration;
    set -x
}

# there's a runtime inside but not once exited
wait_for_spfs_monitor_count 0
assert_runtime_count 0
inner_count=$(spfs run - -- spfs runtime list -q | wc -l)
test $inner_count -eq 1
wait_for_spfs_monitor_count 0
assert_runtime_count 0

# many runtimes at once
spfs run - -- sleep 6 &
spfs run - -- sleep 6 &
spfs run - -- sleep 6 &
spfs run - -- sleep 6 &
# wait for them all to spin up
sleep 4
assert_runtime_count 4
wait_for_spfs_monitor_count 0
assert_runtime_count 0

# many runtimes launched recursively
spfs run - -- spfs run - -- spfs run - -- spfs run - -- sleep 8 &
# when runtimes are stacked, the commands each move into
# a new namespace and so the outer runtimes become empty
# and can be cleaned up immediately

# wait for them all to spin up
sleep 1

# the outer runtimes are cleaned up. Retry aggressively to
# not miss the window where the count is 1.
wait_for_spfs_monitor_count 1 0.1
# don't `assert_runtime_count 1` here because there is a
# race between seeing the monitor count as 1 and that
# monitor exiting and removing the runtime. It is possible
# the runtime count is now 0. The assert that the runtime
# count is 0 below will prove that this runtime was cleaned
# too.

# then the one remaining runtime is cleaned up
wait_for_spfs_monitor_count 0
assert_runtime_count 0

# fast runtime doesn't linger
spfs run - -- true
wait_for_spfs_monitor_count 0
assert_runtime_count 0

