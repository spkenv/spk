// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::graph::State;
use super::validation::{default_validators, VarRequirementsValidator};
use crate::api;

#[rstest]
fn test_src_package_install_requests_are_not_considered() {
    // validators: List[Any] = default_validators()

    // spec = api.Spec.from_dict(
    //     {
    //         "pkg": "my-pkg/1.0.0/src",
    //         "install": {
    //             "embedded": [{"pkg": "embedded/9.0.0"}],
    //             "requirements": [{"pkg": "dependency/=2"}, {"var": "debug/on"}],
    //         },
    //     }
    // )

    // state = State(
    //     pkg_requests=(
    //         api.PkgRequest.from_dict({"pkg": "my-pkg/=1.0.0/src"}),
    //         api.PkgRequest.from_dict({"pkg": "embedded/=1.0.0"}),
    //         api.PkgRequest.from_dict({"pkg": "dependency/=1"}),
    //     ),
    //     var_requests=tuple(),
    //     options=(("debug", "off"),),
    //     packages=tuple(),
    //     hash_cache=[],
    // )
    // for validator in validators:
    //     msg = "Source package should be valid regardless of requirements"
    //     assert validator.validate(state, spec, spec), msg
    todo!()
}

#[rstest]
fn test_empty_options_can_match_anything() {
    // validator = VarRequirementsValidator()

    // state = State(
    //     pkg_requests=tuple(),
    //     var_requests=tuple(),
    //     # this option is requested to be a specific value in the installed
    //     # spec file, but is empty so should not cause a conflict
    //     options=(("python.abi", ""),),
    //     packages=tuple(),
    //     hash_cache=[],
    // )

    // spec = api.Spec.from_dict(
    //     {
    //         "pkg": "my-package/1.0.0",
    //         "install": {"requirements": [{"var": "python.abi/cp37m"}]},
    //     }
    // )

    // assert validator.validate(
    //     state, spec, spec
    // ), "empty option should not invalidate requirement"

    todo!()
}
