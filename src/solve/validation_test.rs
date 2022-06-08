use std::sync::Arc;

// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::{default_validators, OptionsValidator, VarRequirementsValidator};
use crate::solve::{graph::State, validation::ValidatorT};
use crate::{api, fixtures::*, ident, opt_name, solve, spec};

#[rstest]
fn test_src_package_install_requests_are_not_considered() {
    let validators = default_validators();

    let spec = Arc::new(spec!(
        {
            "pkg": "my-pkg/1.0.0/src",
            "install": {
                "embedded": [{"pkg": "embedded/9.0.0"}],
                "requirements": [{"pkg": "dependency/=2"}, {"var": "debug/on"}],
            },
        }
    ));
    let source = solve::PackageSource::Embedded;

    let state = State::new(
        vec![
            api::PkgRequest::from_ident(
                ident!("my-pkg/1.0.0/src"),
                api::RequestedBy::SpkInternalTest,
            ),
            api::PkgRequest::from_ident(
                ident!("embedded/1.0.0"),
                api::RequestedBy::SpkInternalTest,
            ),
            api::PkgRequest::from_ident(ident!("dependency/1"), api::RequestedBy::SpkInternalTest),
        ]
        .into_iter()
        .collect(),
        vec![],
        vec![],
        vec![(opt_name!("debug").to_owned(), "off".to_string())],
    );
    for validator in validators {
        let msg = "Source package should be valid regardless of requirements";
        assert!(
            validator.validate(&state, &*spec, &source).unwrap().is_ok(),
            "{}",
            msg
        );
    }
}

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
    let source = solve::PackageSource::Embedded; // TODO: ???

    assert!(
        validator.validate(&state, &*spec, &source).unwrap().is_ok(),
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
    let source = solve::PackageSource::Embedded; // TODO: ???

    let compat = validator.validate(&state, &*spec, &source).unwrap();
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
    let source = solve::PackageSource::Embedded; // TODO: ???
    let compat = validator.validate(&state, &*spec, &source).unwrap();
    assert!(
        !compat.is_ok(),
        "qualified var requests should supercede unqualified ones, got: {}",
        compat
    );
}
