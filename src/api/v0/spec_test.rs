// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::Spec;

#[rstest]
fn test_spec_is_valid_with_only_name() {
    let _spec: Spec = serde_yaml::from_str("{pkg: test-pkg}").unwrap();
}

#[rstest]
fn test_explicit_no_sources() {
    let spec: Spec = serde_yaml::from_str("{pkg: test-pkg, sources: []}").unwrap();
    assert!(spec.sources.is_empty());
}
