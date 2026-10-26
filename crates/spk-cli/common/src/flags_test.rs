// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;
use spk_schema::RequestWithOptions;
use spk_schema::foundation::name::OptName;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::ident::{PkgRequestOptionValue, VarRequest};
use spk_schema::option_map::HOST_OPTIONS;
use spk_solve::Solver;

use crate::flags::{DecisionFormatterSettings, SolverToRun, SolverToShow};

#[derive(clap::Parser)]
struct FormatterOpt {
    #[clap(flatten)]
    settings: DecisionFormatterSettings,
}

#[derive(clap::Parser)]
struct SolverOpt {
    #[clap(flatten)]
    solver: crate::flags::Solver,
}

/// Parse solver flags without inheriting environment overrides.
fn solver_flags(args: &[&str]) -> crate::flags::Solver {
    use clap::{CommandFactory, FromArgMatches};

    let matches = SolverOpt::command()
        .mut_args(|arg| arg.env(None::<&str>))
        .try_get_matches_from(args)
        .unwrap();
    SolverOpt::from_arg_matches(&matches).unwrap().solver
}

/// Parse flags without inheriting solver overrides from the test environment.
fn formatter_settings(args: &[&str]) -> DecisionFormatterSettings {
    use clap::{CommandFactory, FromArgMatches};

    let matches = FormatterOpt::command()
        .mut_args(|arg| arg.env(None::<&str>))
        .try_get_matches_from(args)
        .unwrap();
    FormatterOpt::from_arg_matches(&matches).unwrap().settings
}

/// Absent numeric flags preserve configured values, including explicit zeros.
#[rstest]
#[case(0)]
#[case(7)]
fn test_formatter_numeric_config(#[case] value: u8) {
    let config = spk_config::Solver {
        too_long_seconds: value.into(),
        verbosity_increase_limit: value,
        solve_timeout: value.into(),
        long_solve_threshold: value.into(),
        max_frequent_errors: value.into(),
        ..Default::default()
    };
    let effective = formatter_settings(&["spk"]).get_formatter_config(&config);
    assert_eq!(effective.too_long_seconds, u64::from(value));
    assert_eq!(effective.verbosity_increase_limit, value);
    assert_eq!(effective.solve_timeout, u64::from(value));
    assert_eq!(effective.long_solve_threshold, u64::from(value));
    assert_eq!(effective.max_frequent_errors, usize::from(value));
}

/// Explicit numeric CLI overrides can disable configured settings with zero.
#[test]
fn test_formatter_numeric_cli_overrides() {
    let settings = formatter_settings(&[
        "spk",
        "--increase-verbosity",
        "0",
        "--max-verbosity-increase-level",
        "0",
        "--timeout",
        "0",
        "--long-solves",
        "0",
        "--max-frequent-errors",
        "0",
    ]);
    let effective = settings.get_formatter_config(&spk_config::Solver::default());
    assert_eq!(effective.too_long_seconds, 0);
    assert_eq!(effective.verbosity_increase_limit, 0);
    assert_eq!(effective.solve_timeout, 0);
    assert_eq!(effective.long_solve_threshold, 0);
    assert_eq!(effective.max_frequent_errors, 0);
}

/// Status bar mode disables verbosity escalation without disabling the timeout.
#[test]
fn test_status_bar_preserves_configured_timeout() {
    let config = spk_config::Solver {
        too_long_seconds: 7,
        solve_timeout: 11,
        ..Default::default()
    };
    let effective = formatter_settings(&["spk", "--status-bar"]).get_formatter_config(&config);
    assert_eq!(effective.too_long_seconds, 0);
    assert_eq!(effective.solve_timeout, 11);
}

/// Each configured impossible check survives absent CLI flags.
#[rstest]
#[case(true, false, false)]
#[case(false, true, false)]
#[case(false, false, true)]
#[case(false, false, false)]
fn test_solver_impossible_checks_from_config(
    #[case] initial: bool,
    #[case] validation: bool,
    #[case] builds: bool,
) {
    let config = spk_config::Solver {
        check_impossible_initial: initial,
        check_impossible_validation: validation,
        check_impossible_builds: builds,
        ..Default::default()
    };
    let solver = solver_flags(&["spk"])
        .get_solver_from_config(&config)
        .unwrap();
    let spk_solve::SolverImpl::Step(solver) = solver else {
        panic!("expected step solver");
    };
    assert_eq!(
        solver.any_impossible_checks_enabled(),
        initial || validation || builds
    );
}

/// Boolean overrides support bare enabling flags, explicit false, and enabling all.
#[rstest]
#[case("--check-impossible-initial", "check_impossible_initial")]
#[case("--check-impossible-validation", "check_impossible_validation")]
#[case("--check-impossible-builds", "check_impossible_builds")]
fn test_solver_impossible_check_overrides(#[case] flag: &str, #[case] field: &str) {
    let mut config = spk_config::Solver::default();
    match field {
        "check_impossible_initial" => config.check_impossible_initial = true,
        "check_impossible_validation" => config.check_impossible_validation = true,
        "check_impossible_builds" => config.check_impossible_builds = true,
        _ => unreachable!(),
    }
    let disabled = format!("{flag}=false");
    for (args, expected) in [
        (vec!["spk", disabled.as_str()], false),
        (vec!["spk", flag], true),
        (
            vec!["spk", disabled.as_str(), "--check-impossible-all"],
            true,
        ),
    ] {
        let solver = solver_flags(&args).get_solver_from_config(&config).unwrap();
        let spk_solve::SolverImpl::Step(solver) = solver else {
            panic!("expected step solver");
        };
        assert_eq!(solver.any_impossible_checks_enabled(), expected);
    }
}

/// Configured solver choices replace the CLI default, including Resolvo.
#[rstest]
#[case("", SolverToRun::All)]
#[case("cli", SolverToRun::Cli)]
#[case("checks", SolverToRun::Checks)]
#[case("all", SolverToRun::All)]
#[case("resolvo", SolverToRun::Resolvo)]
#[case("Resolvo", SolverToRun::Resolvo)]
fn test_solver_choice_from_config(#[case] value: &str, #[case] expected: SolverToRun) {
    let config = spk_config::Solver {
        solver_to_run: value.to_string(),
        ..Default::default()
    };
    let settings = formatter_settings(&["spk"]);
    assert!(settings.solver_to_run.is_none());
    assert!(settings.get_solver_to_run(&config).unwrap() == expected);
}

/// Output selection honors config independently of the solver selection.
#[rstest]
#[case("", SolverToShow::Cli)]
#[case("cli", SolverToShow::Cli)]
#[case("checks", SolverToShow::Checks)]
#[case("resolvo", SolverToShow::Resolvo)]
fn test_solver_output_from_config(#[case] value: &str, #[case] expected: SolverToShow) {
    let config = spk_config::Solver {
        solver_to_show: value.to_string(),
        ..Default::default()
    };
    let settings = formatter_settings(&["spk"]);
    assert!(settings.solver_to_show.is_none());
    assert!(settings.get_solver_to_show(&config).unwrap() == expected);
}

/// Explicit CLI choices override both configured solver choices.
#[test]
fn test_solver_cli_overrides_config() {
    let config = spk_config::Solver {
        solver_to_run: "resolvo".to_string(),
        solver_to_show: "resolvo".to_string(),
        ..Default::default()
    };
    let settings = formatter_settings(&[
        "spk",
        "--solver-to-run",
        "all",
        "--solver-to-show",
        "checks",
    ]);
    assert_eq!(
        settings.get_solver_to_run(&config).unwrap(),
        SolverToRun::All
    );
    assert_eq!(
        settings.get_solver_to_show(&config).unwrap(),
        SolverToShow::Checks
    );
}

/// Environment overrides config, while explicit flags still override environment.
#[test]
fn test_solver_environment_overrides_config() {
    use clap::Parser;

    // Isolate environment overrides from other tests in this process.
    if std::env::var_os("SPK_TEST_SOLVER_OVERRIDES").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &spfstest::current_test_name(
                    module_path!(),
                    "test_solver_environment_overrides_config",
                ),
                "--nocapture",
            ])
            .env("SPK_TEST_SOLVER_OVERRIDES", "1")
            .env("SPK_SOLVER__SOLVER_TO_RUN", "checks")
            .env("SPK_SOLVER__SOLVER_TO_SHOW", "checks")
            .env("SPK_SOLVER_TOO_LONG_SECONDS", "7")
            .env("SPK_SOLVER_VERBOSITY_INCREASE_LIMIT", "7")
            .env("SPK_SOLVER_SOLVE_TIMEOUT", "7")
            .env("SPK_SOLVER_LONG_SOLVE_THRESHOLD", "7")
            .env("SPK_SOLVER_MAX_FREQUENT_ERRORS", "7")
            .env("SPK_SOLVER_CHECK_IMPOSSIBLE_INITIAL", "false")
            .env("SPK_SOLVER_CHECK_IMPOSSIBLE_VALIDATION", "false")
            .env("SPK_SOLVER_CHECK_IMPOSSIBLE_BUILDS", "false")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }

    let config = spk_config::Solver {
        solver_to_run: "resolvo".to_string(),
        solver_to_show: "resolvo".to_string(),
        ..Default::default()
    };
    let settings = FormatterOpt::try_parse_from(["spk"]).unwrap().settings;
    assert_eq!(
        settings.get_solver_to_run(&config).unwrap(),
        SolverToRun::Checks
    );
    assert_eq!(
        settings.get_solver_to_show(&config).unwrap(),
        SolverToShow::Checks
    );
    let effective = settings.get_formatter_config(&config);
    assert_eq!(effective.too_long_seconds, 7);
    assert_eq!(effective.verbosity_increase_limit, 7);
    assert_eq!(effective.solve_timeout, 7);
    assert_eq!(effective.long_solve_threshold, 7);
    assert_eq!(effective.max_frequent_errors, 7);

    let solver_config = spk_config::Solver {
        check_impossible_initial: true,
        check_impossible_validation: true,
        check_impossible_builds: true,
        ..Default::default()
    };
    let flags = SolverOpt::try_parse_from(["spk"]).unwrap().solver;
    let solver = flags.get_solver_from_config(&solver_config).unwrap();
    let spk_solve::SolverImpl::Step(solver) = solver else {
        panic!("expected step solver");
    };
    assert!(!solver.any_impossible_checks_enabled());
    let flags = SolverOpt::try_parse_from(["spk", "--check-impossible-initial"])
        .unwrap()
        .solver;
    let solver = flags.get_solver_from_config(&solver_config).unwrap();
    let spk_solve::SolverImpl::Step(solver) = solver else {
        panic!("expected step solver");
    };
    assert!(solver.any_impossible_checks_enabled());

    let settings = FormatterOpt::try_parse_from([
        "spk",
        "--increase-verbosity",
        "0",
        "--max-verbosity-increase-level",
        "0",
        "--timeout",
        "0",
        "--long-solves",
        "0",
        "--max-frequent-errors",
        "0",
    ])
    .unwrap()
    .settings;
    let effective = settings.get_formatter_config(&config);
    assert_eq!(effective.too_long_seconds, 0);
    assert_eq!(effective.verbosity_increase_limit, 0);
    assert_eq!(effective.solve_timeout, 0);
    assert_eq!(effective.long_solve_threshold, 0);
    assert_eq!(effective.max_frequent_errors, 0);

    let settings =
        FormatterOpt::try_parse_from(["spk", "--solver-to-run", "all", "--solver-to-show", "cli"])
            .unwrap()
            .settings;
    assert_eq!(
        settings.get_solver_to_run(&config).unwrap(),
        SolverToRun::All
    );
    assert_eq!(
        settings.get_solver_to_show(&config).unwrap(),
        SolverToShow::Cli
    );
}

/// Invalid configured choices fail explicitly instead of selecting another solver.
#[test]
fn test_invalid_solver_config_is_reported() {
    let config = spk_config::Solver {
        solver_to_run: "invalid".to_string(),
        solver_to_show: "invalid".to_string(),
        ..Default::default()
    };
    let settings = formatter_settings(&["spk"]);
    assert!(
        settings
            .get_solver_to_run(&config)
            .unwrap_err()
            .to_string()
            .contains("solver.solver_to_run")
    );
    assert!(
        settings
            .get_solver_to_show(&config)
            .unwrap_err()
            .to_string()
            .contains("solver.solver_to_show")
    );
}

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
            index_use: Default::default(),
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
            solver_to_run: Some(solver_to_run),
            solver_to_show: Some(solver_to_show),
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
        check_impossible_initial: None,
        check_impossible_validation: None,
        check_impossible_builds: None,
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

#[tokio::test]
async fn test_parse_request_includes_matching_cli_options() {
    let request_flags = crate::flags::Requests {
        pre: false,
        workspace: crate::flags::Workspace::default(),
    };
    let options_flags = crate::flags::Options {
        no_host: true,
        options: vec![
            "mylib.namespace_style=major_minor".to_string(),
            "other.namespace_style=ignored".to_string(),
        ],
    };
    let repos: &[std::sync::Arc<spk_storage::RepositoryHandle>] = &[];

    let (request, extra_options) = request_flags
        .parse_request("mylib", &options_flags, repos)
        .await
        .unwrap();

    assert!(extra_options.is_empty());

    let RequestWithOptions::Pkg(pkg_request) = request else {
        panic!("expected package request");
    };

    let matching_opt = OptName::new("mylib.namespace_style").unwrap().to_owned();
    assert_eq!(
        pkg_request.options.get(&matching_opt),
        Some(&PkgRequestOptionValue::Complete("major_minor".to_string()))
    );
    assert_eq!(pkg_request.options.len(), 1);

    let unrelated_opt = OptName::new("other.namespace_style").unwrap().to_owned();
    assert!(!pkg_request.options.contains_key(&unrelated_opt));
}
