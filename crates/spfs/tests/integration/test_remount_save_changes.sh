#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

set -o errexit

# test that a removed directory does not show up when running the env later

filepath="/spfs/file1";
to_remove="/spfs/file1";
base_tag="test/dir_removal_base";

spfs run - -- bash -c "touch $filepath && spfs commit layer -t $base_tag"
# removing the file and then disabling edits will cause a remount,
# after which the removal should still persist
spfs run -e $base_tag -- bash -c "rm -r $to_remove && spfs edit --off && test ! -f $filepath"
