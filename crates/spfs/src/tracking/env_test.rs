// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::EnvSpec;

#[rstest]
fn test_env_spec_validation() {
    let spec = EnvSpec::new("one+two").expect("failed to parse env spec");
    assert_eq!(spec.items.len(), 2);
}

#[rstest]
fn test_env_spec_empty() {
    EnvSpec::new("").expect_err("empty spec should be invalid");
}
