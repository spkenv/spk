// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::{default_validators, ValidationSpec};

#[test]
fn test_validation_disabling() {
    let spec: ValidationSpec = serde_yaml::from_str("{disabled: [MustInstallSomething]}").unwrap();
    let configured = spec.configured_validators();
    assert_ne!(configured.len(), default_validators().len());
    assert!(!configured.contains(&super::Validator::MustInstallSomething));
}
