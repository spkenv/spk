// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use pretty_assertions::assert_eq;
use rstest::rstest;
use spk_schema_foundation::opt_name;
use spk_schema_ident::VarRequest;

use super::RecipeOption;
use crate::v1::{BuildCondition, PkgPropagation, WhenBlock, WhenCondition};

#[rstest]
fn test_base_when_inheritance() {
    // options can have a root `when` field
    // that acts as the default value for any other
    // `at*` which is not otherwise specified.

    let option: RecipeOption = serde_yaml::from_str(
        r#"{
        pkg: python/3,
        when: {var: debug/on}
    }"#,
    )
    .unwrap();
    let RecipeOption::Pkg(opt) = option else {
        panic!("expected a package option");
    };

    assert_eq!(
        opt.at_build,
        BuildCondition::Enabled {
            when: WhenBlock::Sometimes {
                conditions: vec![VarRequest::new_with_value(opt_name!("debug"), "on")]
            }
        }
    );

    let propagation = PkgPropagation::Enabled {
        version: None,
        components: Default::default(),
        when: WhenBlock::Sometimes {
            conditions: vec![WhenCondition::Var(VarRequest::new_with_value(
                opt_name!("debug"),
                "on",
            ))],
        },
    };
    assert_eq!(opt.at_runtime, propagation);
    assert_eq!(opt.at_downstream_runtime, propagation);
}

#[rstest]
fn test_at_built_when_conversion() {
    // the atBuild field does not support
    // 'pkg' conditionals, and so we expect to
    // see a warning and ignore the application
    // of such a base when field to the atBuild field

    let option: RecipeOption = serde_yaml::from_str(
        r#"{
        pkg: python/3,
        when: {pkg: gcc/>4}
    }"#,
    )
    .unwrap();
    let RecipeOption::Pkg(opt) = option else {
        panic!("expected a package option");
    };

    assert_eq!(
        opt.at_build,
        BuildCondition::Enabled {
            when: WhenBlock::Always
        },
        "Invalid build condition should still be allowed in the global when field"
    );
}
