// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::sync::Arc;

use rstest::rstest;
use spk_format::{FormatChange, FormatChangeOptions};
use spk_foundation::ident_component::Component;
use spk_foundation::option_map;
use spk_name::{opt_name, PkgName};
use spk_solver_solution::PackageSource;
use spk_spec::{recipe, spec};

use super::DecisionBuilder;
use crate::{graph, Decision};

#[rstest]
fn test_resolve_build_same_result() {
    // building a package and resolving an binary build
    // should both result in the same final state... this
    // ensures that builds are not attempted when one already exists

    let base = graph::State::default();

    let recipe = recipe!({"pkg": "test/1.0.0"});
    let recipe = Arc::new(recipe);
    let build_spec = spec!({"pkg": "test/1.0.0/3I42H3S6"});
    let build_spec = Arc::new(build_spec);
    let source = PackageSource::Embedded; // TODO: ???

    let resolve = Decision::builder(&base).resolve_package(&build_spec, source);
    let build = Decision::builder(&base)
        .build_package(&recipe, &build_spec)
        .unwrap();

    let with_binary = resolve.apply(&base);
    let with_build = build.apply(&base);

    let format_change_options = FormatChangeOptions {
        verbosity: 100,
        level: u64::MAX,
    };

    println!("resolve");
    for change in resolve.changes.iter() {
        println!(
            "{}",
            change.format_change(&format_change_options, Some(&with_binary))
        );
    }
    println!("build");
    for change in build.changes.iter() {
        println!(
            "{}",
            change.format_change(&format_change_options, Some(&with_build))
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
    let recipe = Arc::new(recipe!({
        "pkg": "parent/1.0.0",
        "install": {
          "requirements": [
            {"pkg": "dependency/1.0.0"}
          ]
        }
    }));
    let spec = Arc::new(spec!({
        "pkg": "parent/1.0.0",
        "install": {
          "requirements": [
            {"pkg": "dependency/1.0.0"}
          ]
        }
    }));
    let base = std::sync::Arc::new(super::State::default());

    let resolve_state = DecisionBuilder::new(&base)
        .resolve_package(&spec, PackageSource::Embedded) // TODO: embedded???
        .apply(&base);
    let request = resolve_state
        .get_merged_request(PkgName::new("dependency").unwrap())
        .unwrap();
    assert!(
        request
            .pkg
            .components
            .contains(&Component::default_for_run()),
        "default component should be injected when none specified"
    );

    let build_state = DecisionBuilder::new(&base)
        .build_package(&recipe, &spec)
        .unwrap()
        .apply(&base);
    let request = build_state
        .get_merged_request(PkgName::new("dependency").unwrap())
        .unwrap();
    assert!(
        request
            .pkg
            .components
            .contains(&Component::default_for_run()),
        "default component should be injected when none specified"
    );
}
