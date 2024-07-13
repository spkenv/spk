#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# test that spfs run preserves the value of $TMPDIR in the new environment

expected_value=blah

# tcsh
output=$(env "TMPDIR=$expected_value" spfs run - -- tcsh -c 'echo $TMPDIR')
test "$output" = "$expected_value"

# bash
output=$(env "TMPDIR=$expected_value" spfs run - -- bash -c 'echo $TMPDIR')
test "$output" = "$expected_value"
