// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema_foundation::option_map;
use spk_schema_foundation::option_map::OptionMap;

use crate::{recipe, BuildVariant, Recipe};

#[rstest]
fn test_resolve_options_empty_options() {
    let spec = recipe!({
        "pkg": "test/1.0.0",
    });

    let resolved_options = spec
        .resolve_options(&BuildVariant::Default, &OptionMap::default())
        .unwrap();
    // No options were specified and none has magically appeared.
    assert!(resolved_options.is_empty());
}

#[rstest]
#[case::index_0(0)]
#[case::index_1(1)]
fn test_resolve_options_variant_out_of_range(#[case] index: usize) {
    let spec = recipe!({
        "pkg": "test/1.0.0",
    });

    // Grabbing a non-existent variant should fail.
    assert!(spec
        .resolve_options(&BuildVariant::Variant(index), &OptionMap::default())
        .is_err());
}

#[rstest]
#[case::non_version_range_value("fruit", "banana", "mango")]
#[case::version_range_value("fruit", "1.2.3", "2.3.4")]
fn test_resolve_options_variant_adds_new_var_option(
    #[case] opt_name: &str,
    #[case] default_value: &str,
    #[case] override_value: &str,
) {
    let spec = recipe!({
        "pkg": "test/1.0.0",
        "build": {
            "variants": [
                {
                    opt_name: default_value,
                }
            ]
        },
    });

    // The "default" variant still has empty options.
    let resolved_options = spec
        .resolve_options(&BuildVariant::Default, &OptionMap::default())
        .unwrap();
    // No options were specified and none has magically appeared.
    assert!(resolved_options.is_empty());

    // The first variant is not empty.
    let resolved_options = spec
        .resolve_options(&BuildVariant::Variant(0), &OptionMap::default())
        .unwrap();
    // One option expected.
    assert_eq!(resolved_options.len(), 1);
    let (k, v) = resolved_options.into_iter().next().unwrap();
    assert_eq!(k, opt_name);
    assert_eq!(v, default_value);

    // Now do the same thing but pass in an override for the option.

    let option_override = option_map! { opt_name => override_value };

    // The "default" variant still has empty options.
    let resolved_options = spec
        .resolve_options(&BuildVariant::Default, &option_override)
        .unwrap();
    // No options were specified and none has magically appeared.
    assert!(resolved_options.is_empty());

    // The first variant is not empty.
    let resolved_options = spec
        .resolve_options(&BuildVariant::Variant(0), &option_override)
        .unwrap();
    // One option expected.
    assert_eq!(resolved_options.len(), 1);
    // The override should have won.
    let (k, v) = resolved_options.into_iter().next().unwrap();
    assert_eq!(k, opt_name);
    assert_eq!(v, override_value);
}
