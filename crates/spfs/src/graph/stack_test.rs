// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use super::Stack;
use crate::fixtures::*;

#[rstest]
fn test_stack_deduplication() {
    let expected = [
        random_digest(),
        random_digest(),
        random_digest(),
        random_digest(),
    ];

    let stack = Stack::from_iter([
        expected[0],
        expected[0],
        expected[1],
        expected[2],
        expected[2],
        expected[1],
        expected[2],
        expected[1],
        expected[0],
        expected[3],
        expected[0],
        expected[3],
        // anything previous should be irrelevant
        // to the final order of these layers being
        // added to the stack, since they each replace
        // previous occurrences of themselves
        expected[0],
        expected[1],
        expected[2],
        expected[3],
    ]);

    let actual = stack.iter_bottom_up().collect::<Vec<_>>();
    assert_eq!(actual, expected);
}
