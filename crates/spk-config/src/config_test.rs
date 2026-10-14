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
