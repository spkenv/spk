// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use rstest::rstest;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::opt_name;
use spk_schema::ident::{build_ident, version_ident, PkgRequest, Request, RequestedBy};
use spk_schema::{spec, FromYaml};
use spk_solve::recipe;
use spk_solve_graph::State;
use spk_solve_solution::PackageSource;

use super::{default_validators, OptionsValidator, ValidatorT, VarRequirementsValidator};

#[rstest]
fn test_src_package_install_requests_are_not_considered() {
    // Test for embedded packages in a src package: that a src
    // package/recipe is valid even though one of its requirements is
    // an embedded requirement that does not match the current state.
    // TODO: not sure of this post-spec/package/recipe split
    let validators = default_validators();

    let spec = Arc::new(recipe!(
        {
            "pkg": "my-pkg/1.0.0",
            "install": {
                "embedded": [{"pkg": "embedded/9.0.0"}],
                "requirements": [{"pkg": "dependency/=2"}, {"var": "debug/on"}],
            },
        }
    ));

    let state = State::new(
        vec![
            PkgRequest::from_ident(
                build_ident!("my-pkg/1.0.0/src").to_any(),
                RequestedBy::SpkInternalTest,
            ),
            PkgRequest::from_ident(
                version_ident!("embedded/1.0.0").to_any(None),
                RequestedBy::SpkInternalTest,
            ),
            PkgRequest::from_ident(
                version_ident!("dependency/1").to_any(None),
                RequestedBy::SpkInternalTest,
            ),
        ]
        .into_iter()
        .collect(),
        vec![],
        vec![],
        vec![(opt_name!("debug").to_owned(), "off".to_string())],
    );

    for validator in validators {
        assert!(
            validator.validate_recipe(&*state, &*spec).unwrap().is_ok(),
            "Source package should be valid regardless of requirements but wasn't"
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
            "pkg": "my-package/1.0.0/3I42H3S6",
            "install": {"requirements": [{"var": "python.abi/cp37m"}]},
        }
    ));
    let source = PackageSource::Embedded;

    assert!(
        validator
            .validate_package(&*state, &spec, &source)
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
            Request::from_yaml("{var: debug/off}")
                .unwrap()
                .into_var()
                .unwrap(),
            Request::from_yaml("{var: my-package.debug/on}")
                .unwrap()
                .into_var()
                .unwrap(),
        ],
        vec![],
        vec![],
    );

    // this static value of debug=on should be valid even though it conflicts
    // with the unqualified request for the debug=off
    let spec = Arc::new(spec!(
        {
            "pkg": "my-package/1.0.0/3I42H3S6",
            "build": {"options": [{"var": "debug", "static": "on"}]},
        }
    ));
    let source = PackageSource::Embedded;

    let compat = validator
        .validate_package(&*state, &*spec, &source)
        .unwrap();
    assert!(
        compat.is_ok(),
        "qualified var requests should superseded unqualified ones, got: {compat}"
    );

    // where the above is valid, this spec should fail because debug
    // is set to off and we have the same qualified request for it to
    // be on, even though there is an unqualified request for 'off'
    let spec = Arc::new(spec!(
        {
            "pkg": "my-package/1.0.0/3I42H3S6",
            "build": {"options": [{"var": "debug", "static": "off"}]},
        }
    ));
    let source = PackageSource::Embedded;
    let compat = validator
        .validate_package(&*state, &*spec, &source)
        .unwrap();
    assert!(
        !compat.is_ok(),
        "qualified var requests should supersede unqualified ones, got: {compat}",
    );
}
