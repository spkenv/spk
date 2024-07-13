#!/bin/bash -v

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# test that an edited file saved in a platform is present in future
# environments

filename="/spfs/message.txt";
base_tag="test/file_edit_base";
platform_tag="test/file_edit_top";

spfs run - -- bash -c "echo hello > $filename && spfs commit layer -t $base_tag"
spfs run -e $base_tag -- bash -c "echo edited > $filename && spfs commit platform -t $platform_tag"
spfs run $platform_tag -- grep edited $filename
