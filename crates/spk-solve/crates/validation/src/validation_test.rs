// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashMap;
use std::sync::Arc;

use rstest::rstest;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::opt_name;
use spk_schema::ident::{ident, PkgRequest, Request, RequestedBy};
use spk_schema::name::PkgNameBuf;
use spk_schema::{spec, FromYaml};
use spk_solve::{make_build, make_repo, recipe};
use spk_solve_graph::State;
use spk_solve_solution::PackageSource;

use super::{
    default_validators,
    ImpossibleRequestsChecker,
    OptionsValidator,
    ValidatorT,
    VarRequirementsValidator,
};

#[rstest]
fn test_src_package_install_requests_are_not_considered() {
    // Test for embedded packages in a src package: that a src
    // package/recipe is valid even though one of its requirements is
    // an embedded requirement that does not match the current state.
    // TODO: not sure of this post-spec/package/recipe split
    let validators = default_validators();

    let spec = Arc::new(recipe!(
        {
            "pkg": "my-pkg/1.0.0/src",
            "install": {
                "embedded": [{"pkg": "embedded/9.0.0"}],
                "requirements": [{"pkg": "dependency/=2"}, {"var": "debug/on"}],
            },
        }
    ));

    let state = State::new(
        vec![
            PkgRequest::from_ident(ident!("my-pkg/1.0.0/src"), RequestedBy::SpkInternalTest),
            PkgRequest::from_ident(ident!("embedded/1.0.0"), RequestedBy::SpkInternalTest),
            PkgRequest::from_ident(ident!("dependency/1"), RequestedBy::SpkInternalTest),
        ]
        .into_iter()
        .collect(),
        vec![],
        vec![],
        vec![(opt_name!("debug").to_owned(), "off".to_string())],
    );

    for validator in validators {
        assert!(
            validator.validate_recipe(&state, &*spec).unwrap().is_ok(),
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
            .validate_package(&state, &spec, &source)
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

    let compat = validator.validate_package(&state, &*spec, &source).unwrap();
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
    let compat = validator.validate_package(&state, &*spec, &source).unwrap();
    assert!(
        !compat.is_ok(),
        "qualified var requests should supersede unqualified ones, got: {compat}",
    );
}

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_set_binary() {
    init_logging();

    let request = PkgRequest::from_ident(ident!("my-pkg/1"), RequestedBy::SpkInternalTest);

    let repo = make_repo!(
        [
            {
                "pkg": "my-pkg/1.0.0/src",
                "deprecated": false,
                "build": {"options": [], "script": "echo BUILD"},
            },
            {
                "pkg": "my-pkg/1.0.0",
                "deprecated": true,
                "build": {"options": [], "script": "echo BUILD"},
            },
        ],
        options={}
    );
    let arc_repo = Arc::new(repo);

    let mut requests_checker = ImpossibleRequestsChecker::default();

    // Ensure a binary only checker is added
    requests_checker.set_binary_only(true);

    // Test: with a binary only checker there should be no valid
    // builds for this request
    let result = requests_checker
        .any_build_valid_for_request(&request, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        !result,
        "There should not be valid build for this request (binary checker set: true) but there was"
    );

    // Add a binary only checker again, even though one was just added
    requests_checker.set_binary_only(true);

    // Test: with a binary only checker added again, there should be
    // no valid builds for this request
    let result = requests_checker
        .any_build_valid_for_request(&request, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        !result,
        "There should not be valid build for this request (binary checker set: true x 2) but there was",
    );

    // Ensure a binary only checker is removed
    requests_checker.set_binary_only(false);

    // Test: with the binary only checker removed there should be a
    // valid build for this request
    let result = requests_checker
        .any_build_valid_for_request(&request, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        result,
        "There should be valid build for this request (binary checker set: false) but there wasn't",
    );

    // Ensure a binary only checker is added again
    requests_checker.set_binary_only(true);

    // Test: with the binary only checker re-added there should be no
    // valid builds for this request
    let result = requests_checker
        .any_build_valid_for_request(&request, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        !result,
        "There should not be valid build for this request (binary checker set: true, again) but there was",
    );

    // Test: with the binary only checker still present, and a non-src
    // build in the repo, there should be a valid build for the request
    let repo2 = make_repo!(
        [
            { "pkg": "my-pkg/1.0.0/src" },
            { "pkg": "my-pkg/1.0.0" }
        ]
    );
    let arc_repo4 = Arc::new(repo2);

    let result = requests_checker
        .any_build_valid_for_request(&request, &[arc_repo4])
        .await
        .unwrap();

    assert!(
        result,
        "There should be valid build for this request (binary checker set: still true)"
    );
}

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_any_build_valid_for_request_valid_build() {
    init_logging();

    let request = PkgRequest::from_ident(ident!("my-pkg/1"), RequestedBy::SpkInternalTest);
    let repo = make_repo!(
        [
            { "pkg": "my-pkg/2.0.0" },
            { "pkg": "my-pkg/2.0.0/src" },
            { "pkg": "my-pkg/1.0.0" },
            { "pkg": "my-pkg/1.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    // Test: a request that should have a valid build in the repo
    let mut requests_checker = ImpossibleRequestsChecker::default();
    requests_checker.set_binary_only(true);
    let result = requests_checker
        .any_build_valid_for_request(&request, &[arc_repo])
        .await
        .unwrap();

    assert!(
        result,
        "There should be valid build for this request but there wasn't"
    );
}

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_any_build_valid_for_request_no_valid_build() {
    init_logging();

    let request = PkgRequest::from_ident(ident!("my-pkg/7.0.0"), RequestedBy::SpkInternalTest);
    let repo = make_repo!(
        [
            { "pkg": "my-pkg/2.0.0" },
            { "pkg": "my-pkg/2.0.0/src" },
            { "pkg": "my-pkg/1.0.0" },
            { "pkg": "my-pkg/1.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    // Test: a request that should not have a valid build in the repo
    let mut requests_checker = ImpossibleRequestsChecker::default();
    requests_checker.set_binary_only(true);
    let result = requests_checker
        .any_build_valid_for_request(&request, &[arc_repo])
        .await
        .unwrap();

    assert!(
        !result,
        "There should not be valid build for this request but there was"
    );
}

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_validate_pkg_requests_possible() {
    init_logging();

    let repo = make_repo!(
        [
            { "pkg": "new-pkg/2.0.0" },
            { "pkg": "new-pkg/2.0.0/src" },

            { "pkg": "my-pkg/2.0.0" },
            { "pkg": "my-pkg/1.0.0" },
            { "pkg": "my-pkg/1.0.0/src" },

            { "pkg": "my-pkg-b/3.0.0" },
            { "pkg": "my-pkg-b/3.1.1" },
            { "pkg": "my-pkg-b/3.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    let spec = spec!(
        { "pkg": "about-to-resolve/1.0.0",
           "install": {
               "requirements": [{"pkg": "my-pkg/1.0.0"}, {"pkg": "my-pkg-b/3.1.1"}],
           }
        }
    );

    let unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();

    // Test: a package that adds only possible requests
    let mut requests_checker = ImpossibleRequestsChecker::default();
    requests_checker.set_binary_only(true);
    let compat = requests_checker
        .validate_pkg_requests(&spec, &unresolved_requests, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        compat.is_ok(),
        "Should not make an impossible request but it has somehow"
    );

    // Test: the same kind of request appearing later in a solve. This
    // exercises the possible requests caching for coverage.
    let compat_possible = requests_checker
        .validate_pkg_requests(&spec, &unresolved_requests, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        compat_possible.is_ok(),
        "Should not make an impossible request but it has somehow"
    );
}

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_validate_pkg_requests_impossible() {
    init_logging();

    let repo = make_repo!(
        [
            { "pkg": "new-pkg/2.0.0" },
            { "pkg": "new-pkg/2.0.0/src" },

            { "pkg": "my-pkg/2.0.0" },
            { "pkg": "my-pkg/1.0.0" },
            { "pkg": "my-pkg/1.0.0/src" },

            { "pkg": "my-pkg-b/3.0.0" },
            { "pkg": "my-pkg-b/3.1.1" },
            { "pkg": "my-pkg-b/3.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    // Thie first request is the impossible one. The IfAlreadyPresent
    // request is possible because it is IfAlreadyPresent and may not
    // be requested by anything else
    let spec = spec!(
        { "pkg": "about-to-resolve/1.0.0",
           "install": {
               "requirements": [{"pkg": "my-pkg/7.0.0"},
                                {"pkg": "my-pkg-b/3.1.1"},
                                {"pkg": "not-present/1.0.0",
                                 "include": "IfAlreadyPresent"},
                                {"var": "python.abi/3.9.7"}],
           }
        }
    );

    let unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();

    // Test: a package that adds an impossible request
    let mut requests_checker = ImpossibleRequestsChecker::default();
    requests_checker.set_binary_only(true);
    let compat = requests_checker
        .validate_pkg_requests(&spec, &unresolved_requests, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        !compat.is_ok(),
        "Should make an impossible request but it hasn't"
    );

    // Test: another package that would add the same impossible request.
    // This is for test coverage and should exercise the
    // ImpossibleRequestsChecker's cache.
    let spec2 = spec!(
        { "pkg": "another-to-resolve/2.0.0",
           "install": {
               "requirements": [{"pkg": "my-pkg/7.0.0"}, {"pkg": "my-pkg-b/3.1.1"}],
           }
        }
    );

    let compat = requests_checker
        .validate_pkg_requests(&spec2, &unresolved_requests, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        !compat.is_ok(),
        "Should make an impossible request but it hasn't"
    );
}

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_validate_pkg_requests_no_requirements() {
    init_logging();

    let repo = make_repo!(
        [
            { "pkg": "my-pkg/2.0.0" },
        ]
    );
    let arc_repo = Arc::new(repo);

    let spec = make_build!(
        { "pkg": "about-to-resolve/1.0.0",
           "install": {
               "requirements": []
           }
        }
    );

    let request = PkgRequest::from_ident(ident!("my-pkg/2"), RequestedBy::SpkInternalTest);

    let mut unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();
    unresolved_requests.insert(request.pkg.name.clone(), request.clone());

    // Test: a package that has no install requirements of its own
    let mut requests_checker = ImpossibleRequestsChecker::default();
    requests_checker.set_binary_only(true);
    let compat = requests_checker
        .validate_pkg_requests(&spec, &unresolved_requests, &[arc_repo])
        .await
        .unwrap();

    assert!(
        compat.is_ok(),
        "Should not make an impossible request but it has somehow"
    );
}

#[rstest]
fn test_impossible_requests_checker_reset() {
    let mut requests_checker = ImpossibleRequestsChecker::default();

    // Test: calling reset
    requests_checker.reset();

    assert!(
        requests_checker.num_ifalreadypresent_requests == 0
            && requests_checker.num_impossible_requests_found == 0
            && requests_checker.num_possible_requests_found == 0
            && requests_checker.num_impossible_cache_hits == 0
            && requests_checker.num_possible_cache_hits == 0,
        "Reset should have zeroed out the counters, but didn't"
    );
}

#[rstest]
fn test_impossible_requests_get_impossible_requests() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: getting impossible requests cache
    let requests = requests_checker.get_impossible_requests();

    assert!(requests.is_empty(), "Impossible requests should be empty");
}

#[rstest]
fn test_impossible_requests_get_possible_requests() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: getting possible requests cache
    let requests = requests_checker.get_possible_requests();

    assert!(requests.is_empty(), "Possible requests should be empty");
}

#[rstest]
fn test_impossible_requests_get_num_ifalreadypresent_requests() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of IfAlreadyPresent requests
    let number = requests_checker.get_num_ifalreadypresent_requests();

    assert!(number == 0, "IfAlreadyPresent counter should be zero");
}

#[rstest]
fn test_impossible_requests_get_num_impossible_requests_found() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of impossible requests
    let number = requests_checker.get_num_impossible_requests_found();

    assert!(number == 0, "Impossible requests counter should be zero");
}

#[rstest]
fn test_impossible_requests_get_num_possible_requests_found() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of possible requests
    let number = requests_checker.get_num_possible_requests_found();

    assert!(number == 0, "Possible requests counter should be zero");
}

#[rstest]
fn test_impossible_requests_get_num_impossible_hits() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of impossible cache hits requests
    let number = requests_checker.get_num_impossible_hits();

    assert!(number == 0, "Impossible cache hits counter should be zero");
}

#[rstest]
fn test_impossible_requests_get_num_possible_hits() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of possible cache hits requests
    let number = requests_checker.get_num_possible_hits();

    assert!(number == 0, "Possible cache hits counter should be zero");
}

#[rstest]
fn test_impossible_requests_get_build_specs_read() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of builds specs read
    let number = requests_checker.get_num_build_specs_read();

    assert!(number == 0, "Builds specs read counter should be zero");
}
