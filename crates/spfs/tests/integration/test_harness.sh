#!/bin/bash

# Copyright (c) Sony Pictures Imageworks, et al.
# SPDX-License-Identifier: Apache-2.0
# https://github.com/imageworks/spk

run_tests_in_dir() {
  local tests_directory="$1"
  local num_tests=0
  local num_errors=0

  tests_directory="$(cd "$tests_directory" &> /dev/null && pwd)"

  for file in $(ls "$tests_directory"/test_*.sh); do
    if (( num_tests > 0 )); then
      # sleep between tests
      sleep 1
    fi

    ((num_tests++))

    echo running test: $file
    echo "-----------------------------"

    bash -ex "$file"
    result="$?"

    if [[ "$result" -eq 0 ]]; then
      echo "----------- OK --------------"
      continue
    fi

    echo test failed: $file
    ((num_errors++))
    echo "--------- FAILED ------------"
  done

  if (( num_errors > 0 )); then
    local test_or_tests="test"
    if (( num_tests > 1 )); then
      test_or_tests="tests"
    fi
    echo "$num_errors of $num_tests $test_or_tests failed."
    return 1
  fi

  echo "All tests passed!"
  return 0
}