// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

/// Legacy TOML configs remain loadable through the shared config builder path.
#[test]
fn test_legacy_solver_use_indexes_is_ignored() {
    let builder = config::Config::builder().add_source(config::File::from_str(
        r#"
[solver]
use_indexes = true
solver_to_run = "checks"

[solver.indexes]
verify_before_use = false
kind = "legacy"

[repositories.origin]
use_index = false
[repositories.origin.index]
verify_before_use = true
"#,
        config::FileFormat::Toml,
    ));
    let config = super::Config::from_builder(builder).unwrap();

    assert_eq!(config.solver.solver_to_run, "checks");
    assert!(!config.repositories["origin"].use_index);
    assert!(config.repositories["origin"].index.verify_before_use);
    let serialized = serde_json::to_value(&config).unwrap();
    assert!(serialized["solver"].get("use_indexes").is_none());
    assert!(serialized["solver"].get("indexes").is_none());
}

/// Unspecified numeric settings keep the established CLI defaults after deserialization.
#[test]
fn test_solver_defaults_from_toml() {
    for toml in ["", "[solver]\nsolver_to_run = \"cli\""] {
        let config = super::Config::from_builder(
            config::Config::builder()
                .add_source(config::File::from_str(toml, config::FileFormat::Toml)),
        )
        .unwrap();
        assert_eq!(config.solver.too_long_seconds, 30);
        assert_eq!(config.solver.verbosity_increase_limit, 2);
        assert_eq!(config.solver.solve_timeout, 0);
        assert_eq!(config.solver.long_solve_threshold, 15);
        assert_eq!(config.solver.max_frequent_errors, 15);
    }
}

/// Explicit zero settings are not replaced with defaults by the config loader.
#[test]
fn test_solver_explicit_zeros_from_toml() {
    let config =
        super::Config::from_builder(config::Config::builder().add_source(config::File::from_str(
            r#"
[solver]
too_long_seconds = 0
verbosity_increase_limit = 0
solve_timeout = 0
long_solve_threshold = 0
max_frequent_errors = 0
"#,
            config::FileFormat::Toml,
        )))
        .unwrap();
    assert_eq!(config.solver.too_long_seconds, 0);
    assert_eq!(config.solver.verbosity_increase_limit, 0);
    assert_eq!(config.solver.solve_timeout, 0);
    assert_eq!(config.solver.long_solve_threshold, 0);
    assert_eq!(config.solver.max_frequent_errors, 0);
}
