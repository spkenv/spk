#!/bin/bash

# Copyright (c) Contributors to the SPK project.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/spkenv/spk

set -o errexit

# This test verifies that an editable runtime can write a new file to the mounted filesystem.

# Define variables
BASE_TAG="test/editable_runtime_write_base"
EDITABLE_TAG="test/editable_runtime_write_editable"
ORIGINAL_FILE="/spfs/original.txt"
NEW_FILE="/spfs/new_file.txt"
NEW_FILE_CONTENT="Hello from editable runtime!"

# 1. Create a base layer with an original file
spfs run - -- bash -c "\
  echo \"Original content\" > ${ORIGINAL_FILE} && \
  spfs commit layer -t ${BASE_TAG}\
"

# 2. Run an editable runtime and write a new file
spfs run -e ${BASE_TAG} --edit -- bash -c "\
  echo \"${NEW_FILE_CONTENT}\" > ${NEW_FILE} && \
  test -f ${ORIGINAL_FILE} && \
  test -f ${NEW_FILE} && \
  cat ${NEW_FILE} | grep -q \"${NEW_FILE_CONTENT}\" && \
  spfs commit layer -t ${EDITABLE_TAG}\
"

# 3. Verify the new file exists in the committed editable layer
spfs run ${EDITABLE_TAG} -- bash -c "\
  test -f ${ORIGINAL_FILE} && \
  test -f ${NEW_FILE} && \
  cat ${NEW_FILE} | grep -q \"${NEW_FILE_CONTENT}\"\
"

# 4. Verify the original file still exists and has its original content
spfs run ${EDITABLE_TAG} -- bash -c "\
  test -f ${ORIGINAL_FILE} && \
  cat ${ORIGINAL_FILE} | grep -q \"Original content\"\
"

echo "All tests passed!"
