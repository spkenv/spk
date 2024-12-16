// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use itertools::Itertools;

use super::ValidationSpec;
use crate::LintedItem;

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

#[test]
fn test_validation_rule_lints() {
    let spec: LintedItem<ValidationSpec> =
        serde_yaml::from_str("{rule: [{allow: RecursiveBuild}], disable: []}").unwrap();

    assert_eq!(spec.lints.len(), 2);
    let keys: Vec<String> = spec
        .lints
        .iter()
        .map(|key| key.get_key().to_string())
        .collect_vec();

    assert!(keys.contains(&"validation.rule".to_string()));
    assert!(keys.contains(&"validation.disable".to_string()));
}
