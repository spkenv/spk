// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::borrow::Cow;

use rstest::rstest;
use spk_solve_macros::{make_repo, request};

use super::Solver;

#[rstest]
#[tokio::test]
async fn basic() {
    let repo = make_repo!(
        [
            {"pkg": "basic/1.0.0"},
        ]
    );

    let mut solver = Solver::new(vec![repo.into()], Cow::Borrowed(&[]));
    let solution = solver.solve(&[request!("basic")]).await.unwrap();
    assert_eq!(solution.len(), 1);
}
