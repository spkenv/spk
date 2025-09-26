// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema::foundation::name::OptName;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::VarRequest;
use spk_schema::option_map::HOST_OPTIONS;
use spk_solve::Solver;

use crate::flags::{DecisionFormatterSettings, SolverToRun, SolverToShow};

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

#[rstest]
#[case::cli(SolverToRun::Cli, SolverToShow::Cli)]
#[case::cli(SolverToRun::Checks, SolverToShow::Checks)]
#[case::cli(SolverToRun::Resolvo, SolverToShow::Resolvo)]
#[tokio::test]
async fn test_get_solver_with_host_options(
    #[case] solver_to_run: SolverToRun,
    #[case] solver_to_show: SolverToShow,
    #[values(true, false)] no_host: bool,
) {
    // Test the get_solver() method adds the host options to the solver
    // correctly.

    use std::collections::HashSet;

    let options_flags = crate::flags::Options {
        options: Vec::new(),
        no_host,
    };

    let solver_flags = crate::flags::Solver {
        repos: crate::flags::Repositories {
            local_repo_only: false,
            no_local_repo: false,
            enable_repo: Default::default(),
            disable_repo: Default::default(),
            when: None,
            wrap_origin: None,
        },
        decision_formatter_settings: DecisionFormatterSettings {
            time: Default::default(),
            increase_verbosity: Default::default(),
            max_verbosity_increase_level: Default::default(),
            timeout: Default::default(),
            show_solution: Default::default(),
            long_solves: Default::default(),
            max_frequent_errors: Default::default(),
            status_bar: Default::default(),
            solver_to_run,
            solver_to_show,
            show_search_size: Default::default(),
            compare_solvers: Default::default(),
            stop_on_block: Default::default(),
            step_on_block: Default::default(),
            step_on_decision: Default::default(),
            output_to_dir: Default::default(),
            output_to_dir_min_verbosity: Default::default(),
            output_file_prefix: Default::default(),
        },
        allow_builds: false,
        check_impossible_initial: false,
        check_impossible_validation: false,
        check_impossible_builds: false,
        check_impossible_all: false,
    };

    let solver = solver_flags.get_solver(&options_flags).await.unwrap();
    let var_requests = solver
        .get_var_requests()
        .into_iter()
        .collect::<HashSet<_>>();

    assert!(
        !HOST_OPTIONS.get().unwrap().is_empty(),
        "HOST_OPTIONS must not be empty for this test to be meaningful"
    );

    for (name, value) in HOST_OPTIONS.get().unwrap() {
        let var_request = VarRequest::new_with_value(name, value);
        if no_host {
            assert!(!var_requests.contains(&var_request));
        } else {
            assert!(var_requests.contains(&var_request));
        }
    }
}
