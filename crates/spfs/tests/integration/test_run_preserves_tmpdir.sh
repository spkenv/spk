#!/bin/bash

# Copyright (c) 2022 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# test that spfs run preserves the value of $TMPDIR in the new environment

expected_value=blah

# tcsh
output=$(env "TMPDIR=$expected_value" spfs run - -- tcsh -c 'echo $TMPDIR')
test "$output" = "$expected_value"

# bash
output=$(env "TMPDIR=$expected_value" spfs run - -- bash -c 'echo $TMPDIR')
test "$output" = "$expected_value"