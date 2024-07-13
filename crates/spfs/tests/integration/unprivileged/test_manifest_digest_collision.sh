#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# test that an empty platform's digest doesn't collide with a specially crafted
# blob

# Commit an empty platform. At the time of writing with the current digest
# calculation strategy, this should produce a digest of
# V5KXB5NBQEFXV54MV5F4OCTGB4G7KHSCXL4R2TPFWIZI3YHIHX6A====
spfs run - -- bash -c "spfs commit --allow-empty platform -t test/empty_platform"

# Commit a blob containing 8 null bytes. This blob will also have the digest
# V5KXB5NBQEFXV54MV5F4OCTGB4G7KHSCXL4R2TPFWIZI3YHIHX6A====
spfs run - -- bash -c "dd if=/dev/zero bs=1 count=8 2>/dev/null | spfs write -t test/blob"

# It should be possible to `spfs read` the blob; this command fails if the
# object is not a blob.
spfs read test/blob

# Reading the blob should succeed and have the expected contents.
expected_hash=$(dd if=/dev/zero bs=1 count=8 2>/dev/null | sha256sum | cut -d' ' -f1)
actual_hash=$(spfs read test/blob 2>/dev/null | sha256sum | cut -d' ' -f1)

if [ "$expected_hash" != "$actual_hash" ]; then
    echo "Expected hash $expected_hash but got $actual_hash"
    exit 1
fi
