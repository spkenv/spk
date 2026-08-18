#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# A runtime whose entry command is a setuid binary is never cleaned up.
#
# spfs-monitor identifies the runtime's mount namespace by reading
# /proc/<owner>/ns/mnt. For a non-dumpable process that read does not fail the
# way the code expects -- it SUCCEEDS and yields an empty string:
#
#     readlink("/proc/<non-dumpable>/ns/mnt") -> ''      (not EACCES)
#
# so `identify_mount_namespace_of_process` returns Some("") and the monitor
# adopts "" as this runtime's namespace identity rather than failing.
#
# A setuid entry command makes that deterministic rather than racy: the payload
# is non-dumpable from its own exec, roughly 30ms before the monitor reads.
#
# What follows depends on whether anything else on the host also reports an
# empty namespace link, because those now compare equal to the runtime:
#
#   - if something does, it is treated as a member of the runtime forever and
#     the runtime is never removed;
#   - if nothing does, the tracked set is empty from the start and the monitor
#     tears the runtime down while the command is still running.
#
# A workstation always has the first case (hundreds of non-dumpable processes,
# kernel threads included). A CI container with its own pid namespace can have
# none, which made this test environment-dependent. So rather than depend on
# ambient processes, the test creates exactly one of its own: a setuid sentinel
# started OUTSIDE any runtime. It reports an empty namespace link, so a monitor
# holding the "" identity adopts it as a member of a runtime it was never in,
# and the leak is deterministic on any host.
#
# Both directions are asserted. "The runtime is gone" is not on its own
# evidence of correctness -- it is also what premature teardown looks like.
#
# This test must run privileged because it needs root to create the setuid
# binaries, but spfs itself must run as an unprivileged user: a root monitor
# could read the payload's namespace and none of this would reproduce.

SPFS=/usr/local/bin/spfs
TEST_USER=${SPFS_TEST_USER:-user1}
SETUID_SLEEP=/usr/local/bin/spfs-test-setuid-sleep
SENTINEL=/usr/local/bin/spfs-test-setuid-sentinel
SENTINEL_SECS=300
PAYLOAD_SECS=20

# Keep runtime storage private to this test so runtime counts are unambiguous.
export SPFS_STORAGE_ROOT=/tmp/spfs-repos-setuid-cleanup
rm -rf $SPFS_STORAGE_ROOT
mkdir -p $SPFS_STORAGE_ROOT
chmod 777 $SPFS_STORAGE_ROOT
MONITOR_LOG=$SPFS_STORAGE_ROOT/monitor-trace.log

runtime_count() {
    $SPFS runtime list -q 2>/dev/null | wc -l
}

# Match the process-listing idiom used by test_runtime_cleanup.sh; skip
# defunct entries, which github actions' init does not always reap.
pids_matching() {
    ps -eo pid,args | grep -- "$1" | grep -v grep | grep -v defunct \
        | awk '{print $1}'
}

# Bounded wait -- the whole point of this test is that the count may never
# reach the expected value, so an unbounded loop would hang the suite.
wait_for_runtime_count() {
    local expected=$1
    local timeout=${2:-30}
    local elapsed=0
    while test "$(runtime_count)" -ne "$expected"; do
        if test $elapsed -ge $timeout; then
            return 1
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done
    return 0
}

cleanup() {
    # This test deliberately produces a monitor that never exits; do not leave
    # it polling /proc forever, and do not leave the runtime behind for later
    # tests to trip over.
    set +o errexit
    for name in $($SPFS runtime list -q 2>/dev/null); do
        # --ignore-user because the runtimes belong to $TEST_USER and this
        # cleanup runs as root; without it the removal is silently declined.
        $SPFS runtime rm --ignore-monitor --ignore-user "$name" >/dev/null 2>&1
    done
    for pid in $(pids_matching "spfs-monitor --runtime-storage file://$SPFS_STORAGE_ROOT"); do
        kill "$pid" >/dev/null 2>&1
    done
    for pid in $(pids_matching "$SENTINEL"); do
        kill "$pid" >/dev/null 2>&1
    done
    rm -f $SETUID_SLEEP $SENTINEL
    rm -rf $SPFS_STORAGE_ROOT
}
trap cleanup EXIT

# Setuid-root copies of /bin/sleep: one to run as the runtime's entry command,
# one to sit outside the runtime as the sentinel. They are placed alongside the
# other spfs binaries because /tmp may be mounted nosuid, which would silently
# defeat the setuid bit and make this test pass for the wrong reason.
for bin in $SETUID_SLEEP $SENTINEL; do
    cp /bin/sleep $bin
    chown root:root $bin
    chmod 4755 $bin
    test -u $bin
done

# Start the sentinel and confirm it gives the monitor exactly the conditions
# this test needs: non-dumpable, and outside any runtime.
sudo -u $TEST_USER $SENTINEL $SENTINEL_SECS >/dev/null 2>&1 &
sentinel_pid=""
for _ in $(seq 1 20); do
    sentinel_pid=$(pids_matching "$SENTINEL" | head -1)
    test -n "$sentinel_pid" && break
    sleep 1
done
test -n "$sentinel_pid"

# As $TEST_USER -- the identity the monitor runs as -- the link reads empty.
sentinel_ns_as_user=$(sudo -u $TEST_USER readlink /proc/$sentinel_pid/ns/mnt || true)
echo "sentinel pid $sentinel_pid namespace link, as $TEST_USER: '$sentinel_ns_as_user'"
test -z "$sentinel_ns_as_user"

# As root the real namespace is readable, and it is the one this test is
# running in -- so the sentinel is definitively outside any spfs runtime.
sentinel_ns_real=$(readlink /proc/$sentinel_pid/ns/mnt)
host_ns=$(readlink /proc/self/ns/mnt)
echo "sentinel true namespace: $sentinel_ns_real (this shell: $host_ns)"
test "$sentinel_ns_real" = "$host_ns"

test "$(runtime_count)" -eq 0

# Control: an ordinary entry command is cleaned up as expected. The sentinel
# cannot disturb this -- a dumpable owner yields a real namespace id, which the
# sentinel's empty one never matches. This proves the assertions below are
# detecting the setuid behaviour and not a broken fixture.
sudo -E -u $TEST_USER $SPFS run - -- /bin/sleep 3
if ! wait_for_runtime_count 0 30; then
    echo "FIXTURE BROKEN: an ordinary runtime was not cleaned up either"
    $SPFS runtime list
    exit 1
fi
echo "control ok: runtime with an ordinary command was cleaned up"

# The reproducer. Run in the background so the runtime can be observed while
# the command is still alive. Trace the monitor so a failure carries its own
# evidence; when the runtime leaks, `spfs-enter --exit` never runs to truncate
# this log.
sudo -E -u $TEST_USER \
    env SPFS_LOG_FILE=$MONITOR_LOG RUST_LOG=spfs=trace \
    $SPFS run - -- $SETUID_SLEEP $PAYLOAD_SECS >/dev/null 2>&1 &
run_pid=$!

appeared=no
for _ in $(seq 1 10); do
    if test "$(runtime_count)" -ge 1; then
        appeared=yes
        break
    fi
    sleep 1
done
sleep 5

# Direction one: the runtime must still exist while its command runs.
if test "$(runtime_count)" -eq 0; then
    if test "$appeared" = "no"; then
        echo "FAILED: no runtime was ever registered for the setuid command"
    else
        echo "FAILED: runtime was torn down while its command was still running"
    fi
    echo "  the command still has ~$((PAYLOAD_SECS - 6))s to run, but /spfs is already gone"
    kill $run_pid 2>/dev/null
    wait $run_pid 2>/dev/null
    exit 1
fi
echo "runtime is present while the setuid command runs, as it should be"

runtime_name=$($SPFS runtime list -q | head -1)
runtime_ns=$($SPFS runtime info "$runtime_name" \
    | sed -n 's/.*"mount_namespace": "\(.*\)".*/\1/p')

wait $run_pid 2>/dev/null

# Direction two: the runtime must be gone once its command has exited.
if wait_for_runtime_count 0 30; then
    echo "runtime with a setuid command was cleaned up"
    exit 0
fi

set +x
echo "FAILED: runtime with a setuid entry command was never cleaned up"
echo "  $(runtime_count) runtime(s) still in storage 30s after the command exited:"
$SPFS runtime list
echo "  runtime namespace, as recorded:  $runtime_ns"
echo "  sentinel namespace, actual:      $sentinel_ns_real"
echo "  live monitors: $(pids_matching "spfs-monitor --runtime-storage file://$SPFS_STORAGE_ROOT" | wc -l)"

if test -s "$MONITOR_LOG"; then
    # the log layer writes ANSI escapes around field names, so strip them first
    stripped=$(sed -e 's/\x1B\[[0-9;]*[a-zA-Z]//g' "$MONITOR_LOG")
    tracked=$(echo "$stripped" | grep -ao 'tracked_processes={[^}]*}' | tail -1)
    if test -n "$tracked"; then
        # the debug format of the tracked set is `{pid: (), pid: (), ...}`
        echo "  monitor reports $(echo "$tracked" | grep -o '[0-9]\+:' | wc -l) tracked pids, expected 0"
        if echo "$tracked" | grep -q "[{ ]$sentinel_pid:"; then
            echo "  and it is tracking the sentinel, pid $sentinel_pid, which has"
            echo "  never been inside this runtime -- it matched only because the"
            echo "  runtime's namespace identity was read as an empty string"
        fi
    else
        # Not evidence that the monitor is stuck: it logs its tracked set on
        # pid events, and on a quiet host the only event may be the watch
        # loop's own 60s timeout -- longer than this test observes for.
        echo "  no tracked_processes line logged yet (the monitor logs on pid"
        echo "  events, which can be up to 60s apart on an idle host)"
    fi
    echo "  --- last 12 lines of the monitor log, clipped to 200 columns ---"
    echo "$stripped" | tail -12 | cut -c1-200 | sed 's/^/  | /'
else
    echo "  (no monitor trace captured at $MONITOR_LOG)"
fi
set -x
exit 1
