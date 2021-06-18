// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Spec;

#[rstest]
fn test_empty_spec_is_valid() {
    let _spec: Spec = serde_yaml::from_str("{}").unwrap();
}

#[rstest]
fn test_explicit_no_sources() {
    let spec: Spec = serde_yaml::from_str("sources: []").unwrap();
    assert!(spec.sources.is_empty());
}
