// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::ops::Deref;

use pretty_assertions::assert_eq;
use rand::seq::SliceRandom;
use rstest::rstest;
use spk_schema_foundation::option_map;
use spk_schema_foundation::option_map::OptionMap;

use super::RecipeOptionList;
use crate::v1::RecipeOption;

#[rstest]
#[case(
    &[],
    option_map!{},
    "{}"
)]
fn test_resolve_options(
    #[case] options: &[&str],
    #[case] given: OptionMap,
    #[case] expected: &str,
) {
    let options: Vec<RecipeOption> = options
        .iter()
        .map(Deref::deref)
        .map(serde_yaml::from_str)
        .map(Result::unwrap)
        .collect();
    let expected: OptionMap = serde_yaml::from_str(expected).unwrap();
    let options = RecipeOptionList(options);
    let actual = options.resolve(&given).expect("Failed to resolve options");
    assert_eq!(actual, expected);
}

#[rstest]
fn test_resolve_options_converges() {
    // options with dependencies on one another should still converge
    // on the same final state of values regardless of the order
    // in which they were specified

    let mut options: Vec<RecipeOption> = serde_yaml::from_str(
        // this is something of a logic sequence, that progresses over time
        // as the default for each var is considered and then set - even
        // though it's not a realistic package example the logic here
        // can be more or less followed in order, but it should resolve
        // the same even when shuffled randomly
        r#"[
        {var: is_battery_charged/no, choices: [yes, no]},
        {var: is_key_turned/no, choices: [yes, no]},

        {var: is_ignition_ready/yes, when: {var: is_battery_charged/yes}},
        {var: is_starter_running/yes, when: [
            {var: is_key_turned/yes},
            {var: is_ignition_ready/yes},
        ]},
        {var: is_cranking/yes, when: {var: is_starter_running/yes}},
        {var: is_running/yes, when: {var: is_cranking/yes}},
    ]"#,
    )
    .unwrap();

    let mut rng = rand::thread_rng();
    for _ in 0..100 {
        options.shuffle(&mut rng);
        let actual = RecipeOptionList(options.clone())
            .resolve(&option_map! {
                "is_battery_charged" => "yes",
                "is_key_turned" => "yes"
            })
            .expect("Should not fail to converge during resolution");
        let expected = option_map! {
            "is_battery_charged" =>"yes",
            "is_key_turned" => "yes",
            "is_ignition_ready" => "yes",
            "is_starter_running" => "yes",
            "is_cranking" => "yes",
            "is_running" => "yes",
        };
        assert_eq!(actual, expected);
    }
}

#[rstest]
fn test_resolve_options_endless() {
    // two active options with different values
    // must generate an error since they are ambiguous.

    let options: RecipeOptionList = serde_yaml::from_str(
        r#"[
            {var: debug/on, when: {var: testing/on}},
            {var: debug/off, when: {var: optimize/on}},
    ]"#,
    )
    .unwrap();
    let actual = options.resolve(&option_map! {
        "optimize" => "on",
        "testing" => "on",
    });
    assert!(
        matches!(
            actual,
            Err(crate::Error::MultipleOptionValuesResolved { .. })
        ),
        "got: {actual:?}"
    );
}
