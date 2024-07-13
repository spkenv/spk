// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use super::ValidationSpec;

#[test]
fn test_validation_rule_expansion() {
    let spec: ValidationSpec = serde_yaml::from_str("{rules: [{allow: RecursiveBuild}]}").unwrap();
    let configured = spec.to_expanded_rules();
    tracing::trace!("{configured:#?}");
    assert!(configured.len() > 1, "Implicit rules should be added");
    assert!(configured.contains(&super::ValidationRule::Allow {
        condition: super::ValidationMatcher::CollectExistingFiles {
            packages: vec![super::NameOrCurrent::Current]
        }
    }));
}
