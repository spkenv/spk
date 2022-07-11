// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use rstest::rstest;

use super::DecisionBuilder;
use crate::{
    api, io, opt_name, option_map,
    solve::{self, graph},
    spec,
};

#[rstest]
fn test_resolve_build_same_result() {
    // building a package and resolving an binary build
    // should both result in the same final state... this
    // ensures that builds are not attempted when one already exists

    let base = graph::State::default();

    let mut build_spec = spec!({"pkg": "test/1.0.0"});
    build_spec
        .update_for_build(&option_map! {}, [] as [&api::Spec; 0])
        .unwrap();
    let build_spec = Arc::new(build_spec);
    let source = solve::PackageSource::Spec(build_spec.clone());

    let resolve =
        solve::graph::Decision::builder(build_spec.clone(), &base).resolve_package(source);
    let build = solve::graph::Decision::builder(build_spec, &base)
        .build_package(&solve::Solution::new(None))
        .unwrap();

    let with_binary = resolve.apply(&base);
    let with_build = build.apply(&base);

    println!("resolve");
    for change in resolve.changes.iter() {
        println!(
            "{}",
            io::format_change(
                change,
                io::FormatChangeOptions {
                    verbosity: 100,
                    level: u64::MAX,
                },
                Some(&with_binary)
            )
        );
    }
    println!("build");
    for change in build.changes.iter() {
        println!(
            "{}",
            io::format_change(
                change,
                io::FormatChangeOptions {
                    verbosity: 100,
                    level: u64::MAX,
                },
                Some(&with_build)
            )
        );
    }

    assert_eq!(
        with_binary.id(),
        with_build.id(),
        "Build and resolve package should create the same final state"
    );
}

#[rstest]
fn test_empty_options_do_not_unset() {
    let state = graph::State::default();

    let assign_empty = graph::SetOptions::new(option_map! {"something" => ""});
    let assign_value = graph::SetOptions::new(option_map! {"something" => "value"});

    let new_state = assign_empty.apply(&state, &state);
    let opts = new_state.get_option_map();
    assert_eq!(
        opts.get(opt_name!("something")),
        Some(String::new()).as_ref(),
        "should assign empty option of no current value"
    );

    let parent = Arc::clone(&new_state);
    let new_state = assign_value.apply(&parent, &new_state);
    let new_state = assign_empty.apply(&parent, &new_state);
    let opts = new_state.get_option_map();
    assert_eq!(
        opts.get(opt_name!("something")),
        Some(String::from("value")).as_ref(),
        "should not unset value when one exists"
    );
}

#[rstest]
fn test_request_default_component() {
    let spec = spec!({
        "pkg": "parent",
        "install": {
          "requirements": [
            {"pkg": "dependency/1.0.0"}
          ]
        }
    });
    let spec = std::sync::Arc::new(spec);
    let base = std::sync::Arc::new(super::State::default());

    let resolve_state = DecisionBuilder::new(spec.clone(), &base)
        .resolve_package(solve::solution::PackageSource::Spec(spec.clone()))
        .apply(&base);
    let request = resolve_state
        .get_merged_request(api::PkgName::new("dependency").unwrap())
        .unwrap();
    assert!(
        request
            .pkg
            .components
            .contains(&api::Component::default_for_run()),
        "default component should be injected when none specified"
    );

    let build_state = DecisionBuilder::new(spec, &base)
        .build_package(&solve::solution::Solution::new(None))
        .unwrap()
        .apply(&base);
    let request = build_state
        .get_merged_request(api::PkgName::new("dependency").unwrap())
        .unwrap();
    assert!(
        request
            .pkg
            .components
            .contains(&api::Component::default_for_run()),
        "default component should be injected when none specified"
    );
}
