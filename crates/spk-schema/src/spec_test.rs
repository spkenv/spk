// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spk_schema_foundation::option_map;
use spk_schema_foundation::option_map::OptionMap;

use crate::{recipe, Recipe};

#[rstest]
fn test_resolve_options_empty_options() {
    let spec = recipe!({
        "pkg": "test/1.0.0",
    });

    let resolved_options = spec.resolve_options(&OptionMap::default()).unwrap();
    // No options were specified and none has magically appeared.
    assert!(resolved_options.is_empty());
}

#[rstest]
#[case::index_0(0)]
#[case::index_1(1)]
fn test_resolve_options_variant_out_of_range(#[case] _index: usize) {
    let spec = recipe!({
        "pkg": "test/1.0.0",
    });

    // Grabbing a non-existent variant should fail.
    assert!(spec
        .resolve_options(/* TODO: resolve for non-existent variant */ &OptionMap::default())
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
    let resolved_options = spec.resolve_options(&OptionMap::default()).unwrap();
    // No options were specified and none has magically appeared.
    assert!(resolved_options.is_empty());

    // The first variant is not empty.
    let resolved_options = spec
        .resolve_options(/* TODO: resolve for variant 0 */ &OptionMap::default())
        .unwrap();
    // One option expected.
    assert_eq!(resolved_options.len(), 1);
    let (k, v) = resolved_options.into_iter().next().unwrap();
    assert_eq!(k, opt_name);
    assert_eq!(v, default_value);

    // Now do the same thing but pass in an override for the option.

    let option_override = option_map! { opt_name => override_value };

    // The "default" variant still has empty options.
    let resolved_options = spec.resolve_options(&option_override).unwrap();
    // No options were specified and none has magically appeared.
    assert!(resolved_options.is_empty());

    // The first variant is not empty.
    let resolved_options = spec
        .resolve_options(/* TODO: resolve for variant 0 */ &option_override)
        .unwrap();
    // One option expected.
    assert_eq!(resolved_options.len(), 1);
    // The override should have won.
    let (k, v) = resolved_options.into_iter().next().unwrap();
    assert_eq!(k, opt_name);
    assert_eq!(v, override_value);
}

macro_rules! assert_option_map_contains {
    ( $option_map:expr, $expected_key:expr, $expected_value:expr ) => {{
        match $option_map.get($crate::opt_name!($expected_key)) {
            None => panic!("option map did not contain expected key {}", $expected_key),
            Some(v) => assert_eq!(v, $expected_value),
        }
    }};
}

#[rstest]
fn test_resolve_options_variant_treated_as_new_pkg() {
    let spec = recipe!({
        "pkg": "test/1.0.0",
        "build": {
            "options": [
                {
                    "pkg": "a-package/1.2.3",
                },
                {
                    "var": "a-var/1.2.3",
                }
            ],
            "variants": [
                // 0
                {
                    "another-package": "2.3.4",
                },
                // 1
                {
                    "a-var": "2.3.4",
                },
                // 2
                {
                    "a-package": "2.3.4",
                }
            ]
        },
    });

    let resolved_options_default = spec.resolve_options(&OptionMap::default()).unwrap();
    let resolved_options_variant_0 = spec
        .resolve_options(/* TODO: resolve for variant 0 */ &OptionMap::default())
        .unwrap();
    let resolved_options_variant_1 = spec
        .resolve_options(/* TODO: resolve for variant 1 */ &OptionMap::default())
        .unwrap();
    let resolved_options_variant_2 = spec
        .resolve_options(/* TODO: resolve for variant 2 */ &OptionMap::default())
        .unwrap();

    // The default baseline...
    assert_option_map_contains!(resolved_options_default, "a-package", "1.2.3");
    assert_option_map_contains!(resolved_options_default, "a-var", "1.2.3");

    // Variant 0...
    assert_option_map_contains!(resolved_options_variant_0, "a-package", "1.2.3");
    assert_option_map_contains!(resolved_options_variant_0, "a-var", "1.2.3");
    assert_option_map_contains!(resolved_options_variant_0, "another-package", "2.3.4");

    // Variant 1...
    assert_option_map_contains!(resolved_options_variant_1, "a-package", "1.2.3");
    // Expect the variant content to match the var in options and override its
    // value.
    assert_option_map_contains!(resolved_options_variant_1, "a-var", "2.3.4");

    // Variant 2...
    // Expect the variant content to match the pkg in options and override its
    // value.
    assert_option_map_contains!(resolved_options_variant_2, "a-package", "2.3.4");
    assert_option_map_contains!(resolved_options_variant_2, "a-var", "1.2.3");
}

macro_rules! assert_requests_contains {
    ( $requests:expr, var, $expected_key:expr, $expected_value:expr, index = $expected_index:expr ) => {{
        if !$requests
            .iter()
            .enumerate()
            .any(|(index, r)| matches!(r, $crate::Request::Var(var) if var.var == $expected_key && var.value == $expected_value && ($expected_index.is_none() || $expected_index.unwrap() == index)))
        {
            panic!(
                "requests did not contain var with {} and {}{}",
                $expected_key, $expected_value, {
                    match $expected_index {
                        Some(index) => format!(" at index {}", index),
                        None => format!(""),
                    }
                }
            );
        }
    }};
    ( $requests:expr, pkg, $expected_key:expr, $expected_value:expr, index = $expected_index:expr ) => {{
        if !$requests
            .iter()
            .enumerate()
            .any(|(index, r)| matches!(r, $crate::Request::Pkg(pkg) if pkg.pkg.name == $expected_key && pkg.pkg.version.to_string() == $expected_value && ($expected_index.is_none() || $expected_index.unwrap() == index)))
        {
            panic!(
                "requests did not contain pkg with {} and {}{}",
                $expected_key, $expected_value, {
                    match $expected_index {
                        Some(index) => format!(" at index {}", index),
                        None => format!(""),
                    }
                }
            );
        }
    }};
    ( $requests:expr, var, $expected_key:expr, $expected_value:expr ) => {{
        assert_requests_contains!($requests, var, $expected_key, $expected_value, index = None::<usize>);
    }};
    ( $requests:expr, pkg, $expected_key:expr, $expected_value:expr ) => {{
        assert_requests_contains!($requests, pkg, $expected_key, $expected_value, index = None::<usize>);
    }};
}

#[rstest]
fn test_get_build_requirements_variant_treated_as_new_pkg() {
    let spec = recipe!({
        "pkg": "test/1.0.0",
        "build": {
            "options": [
                {
                    "pkg": "a-package/1.2.3",
                },
                {
                    "var": "a-var/1.2.3",
                }
            ],
            "variants": [
                // 0
                {
                    "another-package": "2.3.4",
                },
                // 1
                {
                    "a-var": "2.3.4",
                },
                // 2
                {
                    "a-package": "2.3.4",
                }
            ]
        },
    });

    let resolved_options_default = spec.resolve_options(&OptionMap::default()).unwrap();
    let resolved_options_variant_0 = spec
        .resolve_options(/* TODO: resolve for variant 0 */ &OptionMap::default())
        .unwrap();
    let resolved_options_variant_1 = spec
        .resolve_options(/* TODO: resolve for variant 1 */ &OptionMap::default())
        .unwrap();
    let resolved_options_variant_2 = spec
        .resolve_options(/* TODO: resolve for variant 2 */ &OptionMap::default())
        .unwrap();

    // XXX: Is it "cheating" to pass in the resolved options to
    // `get_build_requirements`?

    let build_requirements_default = spec
        .get_build_requirements(&resolved_options_default)
        .unwrap();
    let build_requirements_variant_0 = spec
        .get_build_requirements(/* TODO: build variant 0 */ &resolved_options_variant_0)
        .unwrap();
    let build_requirements_variant_1 = spec
        .get_build_requirements(/* TODO: build variant 1 */ &resolved_options_variant_1)
        .unwrap();
    let build_requirements_variant_2 = spec
        .get_build_requirements(/* TODO: build variant 2 */ &resolved_options_variant_2)
        .unwrap();

    // The default baseline...
    assert_requests_contains!(build_requirements_default, pkg, "a-package", "1.2.3");
    assert_requests_contains!(build_requirements_default, var, "a-var", "1.2.3");

    // Variant 0...
    assert_requests_contains!(build_requirements_variant_0, pkg, "a-package", "1.2.3");
    assert_requests_contains!(build_requirements_variant_0, var, "a-var", "1.2.3");
    assert_requests_contains!(
        build_requirements_variant_0,
        pkg,
        "another-package",
        "2.3.4"
    );

    // Variant 1...
    assert_requests_contains!(build_requirements_variant_1, pkg, "a-package", "1.2.3");
    // Expect the variant content to match the var in options and override its
    // value.
    assert_requests_contains!(build_requirements_variant_1, var, "a-var", "2.3.4");

    // Variant 2...
    // Expect the variant content to match the pkg in options and override its
    // value.
    assert_requests_contains!(build_requirements_variant_2, pkg, "a-package", "2.3.4");
    assert_requests_contains!(build_requirements_variant_2, var, "a-var", "1.2.3");
}

#[rstest]
fn test_get_build_requirements_pkg_in_variant_preserves_order() {
    // The override of `a-package` should not alter the order of the packages
    // as defined in `options`.
    let spec = recipe!({
        "pkg": "test/1.0.0",
        "build": {
            "options": [
                {
                    "pkg": "a-package/1.2.3",
                },
                {
                    "pkg": "b-package/1.2.3",
                },
                {
                    "pkg": "c-package/1.2.3",
                },
                {
                    "var": "a-var/1.2.3",
                }
            ],
            "variants": [
                // 0
                {
                    "a-package": "2.3.4",
                }
            ]
        },
    });

    let resolved_options_variant_0 = spec
        .resolve_options(/* TODO: resolve variant 0 */ &OptionMap::default())
        .unwrap();

    // XXX: Is it "cheating" to pass in the resolved options to
    // `get_build_requirements`?

    let build_requirements_variant_0 = spec
        .get_build_requirements(/* TODO: build variant 0 */ &resolved_options_variant_0)
        .unwrap();

    // Variant 0...
    // Expect the variant content to match the pkg in options and override its
    // value. It is expected to remain in position 0.
    assert_requests_contains!(
        build_requirements_variant_0,
        pkg,
        "a-package",
        "2.3.4",
        index = Some(0)
    );
    assert_requests_contains!(
        build_requirements_variant_0,
        var,
        "a-var",
        "1.2.3",
        index = Some(3)
    );
}
