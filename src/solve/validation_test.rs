use std::sync::Arc;

// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::{default_validators, VarRequirementsValidator};
use crate::solve::{graph::State, validation::ValidatorT};
use crate::{api, ident, solve, spec};

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
    let source = solve::PackageSource::Spec(spec.clone());

    let state = State::new(
        vec![
            api::PkgRequest::from_ident(&ident!("my-pkg/1.0.0/src")),
            api::PkgRequest::from_ident(&ident!("embedded/1.0.0")),
            api::PkgRequest::from_ident(&ident!("dependency/1")),
        ],
        vec![],
        vec![],
        vec![("debug".to_string(), "off".to_string())],
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
        vec![("python.abi".to_string(), "".to_string())],
    );

    let spec = Arc::new(spec!(
        {
            "pkg": "my-package/1.0.0",
            "install": {"requirements": [{"var": "python.abi/cp37m"}]},
        }
    ));
    let source = solve::PackageSource::Spec(spec.clone());

    assert!(
        validator.validate(&state, &*spec, &source).unwrap().is_ok(),
        "empty option should not invalidate requirement"
    );
}
