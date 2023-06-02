#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

## Run all integration tests in the priviledged folder
# these are expected to be run off of the installed spfs binaries
# with the proper capabilities

# Pre-create some users for the tests to use
useradd -m user1
useradd -m user2

. $(dirname "${BASH_SOURCE[0]}")/test_harness.sh

run_tests_in_dir $(dirname "${BASH_SOURCE[0]}")/privileged