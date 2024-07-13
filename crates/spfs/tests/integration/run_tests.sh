#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

## Run all integration tests in unprivileged folder
# these are expected to be run off of the installed spfs binaries
# with the proper capabilities

. $(dirname "${BASH_SOURCE[0]}")/test_harness.sh

run_tests_in_dir $(dirname "${BASH_SOURCE[0]}")/unprivileged
