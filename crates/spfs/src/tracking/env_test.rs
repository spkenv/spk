// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::EnvSpec;

#[rstest]
fn test_env_spec_validation() {
    let spec = EnvSpec::parse("one+two").expect("failed to parse env spec");
    assert_eq!(spec.items.len(), 2);
}

#[rstest]
fn test_env_spec_empty() {
    let empty = EnvSpec::parse("").expect("empty spec should be valid");
    let dash = EnvSpec::parse(super::ENV_SPEC_EMPTY).expect("dash spec should be valid");
    assert_eq!(
        empty, dash,
        "dash and empty string should be an empty spec (for cli parsing)"
    );
}
