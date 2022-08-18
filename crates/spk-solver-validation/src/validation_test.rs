// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use spk_foundation::fixtures::*;
use spk_foundation::opt_name;
use spk_solver_graph::State;
use spk_solver_solution::PackageSource;
use spk_spec::spec;
use std::sync::Arc;

use super::{OptionsValidator, VarRequirementsValidator};
use crate::validation::ValidatorT;

#[rstest]
fn test_empty_options_can_match_anything() {
    let validator = VarRequirementsValidator::default();

    let state = State::new(
        vec![],
        vec![],
        vec![],
        // this option is requested to be a specific value in the installed
        // spec file, but is empty so should not cause a conflict
        vec![(opt_name!("python.abi").to_owned(), "".to_string())],
    );

    let spec = Arc::new(spec!(
        {
            "pkg": "my-package/1.0.0",
            "install": {"requirements": [{"var": "python.abi/cp37m"}]},
        }
    ));
    let source = PackageSource::Embedded;

    assert!(
        validator
            .validate_package(&state, &*spec, &source)
            .unwrap()
            .is_ok(),
        "empty option should not invalidate requirement"
    );
}

#[rstest]
fn test_qualified_var_supersedes_unqualified() {
    init_logging();
    let validator = OptionsValidator::default();

    let state = State::new(
        vec![],
        vec![
            serde_yaml::from_str("{var: debug/off}").unwrap(),
            serde_yaml::from_str("{var: my-package.debug/on}").unwrap(),
        ],
        vec![],
        vec![],
    );

    // this static value of debug=on should be valid even though it conflicts
    // with the unqualified request for the debug=off
    let spec = Arc::new(spec!(
        {
            "pkg": "my-package/1.0.0",
            "build": {"options": [{"var": "debug", "static": "on"}]},
        }
    ));
    let source = PackageSource::Embedded;

    let compat = validator.validate_package(&state, &*spec, &source).unwrap();
    assert!(
        compat.is_ok(),
        "qualified var requests should superseded unqualified ones, got: {}",
        compat
    );

    // where the above is valid, this spec should fail because debug
    // is set to off and we have the same qualified request for it to
    // be on, even though there is an unqualified request for 'off'
    let spec = Arc::new(spec!(
        {
            "pkg": "my-package/1.0.0",
            "build": {"options": [{"var": "debug", "static": "off"}]},
        }
    ));
    let source = PackageSource::Embedded;
    let compat = validator.validate_package(&state, &*spec, &source).unwrap();
    assert!(
        !compat.is_ok(),
        "qualified var requests should supercede unqualified ones, got: {compat}",
    );
}
