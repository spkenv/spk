#!/bin/bash

# Copyright (c) 2021 Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

## Run all integration tests in this folder
# these are expected to be run off of the installed spfs binaries
# with the proper capabilities

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
for file in $(ls $DIR); do
  if [[ "$file" == `basename ${BASH_SOURCE[0]}` ]]; then
    continue
  fi
  echo running test: $file
  echo "-----------------------------"
  bash -ex $DIR/$file
  if [[ $? -ne 0 ]]; then
    echo test failed: $file
    exit 1;
  fi
  echo "----------- OK --------------"
done
