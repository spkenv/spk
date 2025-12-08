#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# This test verifies that spfs shell - followed by spfs edit allows writing files.
# It reproduces the issue where spfs edit shows editable: true but writes fail.

echo "Test: spfs shell - -> spfs edit -> write file"

# 1. Use spfs run to start bash and run spfs edit, then attempt to write
# This simulates the exact workflow: start shell, make editable, write file
spfs run - -- bash -c '
    set -o errexit
    echo "Running spfs edit..."
    spfs edit
    echo "Checking runtime info..."
    spfs info | grep -q "editable: true"
    echo "Attempting to write file..."
    touch /spfs/hello
    echo "Write succeeded!"
    test -f /spfs/hello
'

echo "All tests passed!"