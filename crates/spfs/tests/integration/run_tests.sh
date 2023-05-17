#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

## Run all integration tests in unpriviledged folder
# these are expected to be run off of the installed spfs binaries
# with the proper capabilities

. $(dirname "${BASH_SOURCE[0]}")/test_harness.sh

run_tests_in_dir $(dirname "${BASH_SOURCE[0]}")/unprivileged