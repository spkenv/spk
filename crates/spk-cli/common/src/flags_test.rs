// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema::foundation::name::OptName;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::VarRequest;
use spk_schema::option_map::HOST_OPTIONS;

#[rstest]
#[case(&["hello:world"], &[("hello", "world")])]
#[case(&["hello=world"], &[("hello", "world")])]
#[case(&["{hello: world}"], &[("hello", "world")])]
#[case(&["{python: 2.7}"], &[("python", "2.7")])]
#[case(
    &["{python: 2.7, python.abi: py37m}"],
    &[("python", "2.7"), ("python.abi", "py37m")],
)]
#[should_panic]
#[case(&["{hello: [world]}"], &[])]
#[should_panic]
#[case(&["{python: {v: 2.7}}"], &[])]
#[should_panic]
#[case(&["value"], &[])]
fn test_option_flags_parsing(#[case] args: &[&str], #[case] expected: &[(&str, &str)]) {
    let options = super::Options {
        no_host: true,
        options: args.iter().map(ToString::to_string).collect(),
    };
    let actual = options.get_options().unwrap();
    let expected: OptionMap = expected
        .iter()
        .map(|(k, v)| (OptName::new(k).unwrap().to_owned(), v.to_string()))
        .collect();
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_get_solver_with_host_options() {
    // Test the get_solver() method adds the host options to the solver
    // correctly.

    let options_flags = crate::flags::Options {
        options: Vec::new(),
        no_host: false,
    };

    let solver_flags = crate::flags::Solver {
        repos: crate::flags::Repositories {
            local_repo_only: false,
            no_local_repo: false,
            enable_repo: Default::default(),
            disable_repo: Default::default(),
            when: None,
            legacy_spk_version_tags: false,
        },
        allow_builds: false,
        check_impossible_initial: false,
        check_impossible_validation: false,
        check_impossible_builds: false,
        check_impossible_all: false,
    };

    let solver = solver_flags.get_solver(&options_flags).await.unwrap();
    let initial_state = solver.get_initial_state();

    for (name, value) in HOST_OPTIONS.get().unwrap() {
        let var_request = VarRequest::new_with_value(name, value);
        assert!(initial_state.contains_var_request(&var_request));
    }
}
