// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use rstest::rstest;
use spk_schema::foundation::fixtures::*;
use spk_schema::ident::{version_ident, PkgRequest, RequestedBy};
use spk_schema::name::PkgNameBuf;
use spk_schema::spec;
use spk_solve_macros::{make_build, make_repo};

use super::ImpossibleRequestsChecker;

#[rstest]
#[tokio::test]
async fn test_impossible_requests_checker_set_binary() {
    init_logging();

    let request = PkgRequest::from_ident(
        version_ident!("my-pkg/1").to_any(None),
        RequestedBy::SpkInternalTest,
    );

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

    let requests_checker = ImpossibleRequestsChecker::default();

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

    let request = PkgRequest::from_ident(
        version_ident!("my-pkg/1").to_any(None),
        RequestedBy::SpkInternalTest,
    );
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
    let requests_checker = ImpossibleRequestsChecker::default();
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

    let request = PkgRequest::from_ident(
        version_ident!("my-pkg/7.0.0").to_any(None),
        RequestedBy::SpkInternalTest,
    );
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
    let requests_checker = ImpossibleRequestsChecker::default();
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
            { "pkg": "new-pkg/2.0.0/3I42H3S6" },
            { "pkg": "new-pkg/2.0.0/src" },

            { "pkg": "my-pkg/2.0.0/3I42H3S6" },
            { "pkg": "my-pkg/1.0.0/3I42H3S6" },
            { "pkg": "my-pkg/1.0.0/src" },

            { "pkg": "my-pkg-b/3.0.0/3I42H3S6" },
            { "pkg": "my-pkg-b/3.1.1/3I42H3S6" },
            { "pkg": "my-pkg-b/3.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    let spec = spec!(
        { "pkg": "about-to-resolve/1.0.0/3I42H3S6",
           "install": {
               "requirements": [{"pkg": "my-pkg/1.0.0"}, {"pkg": "my-pkg-b/3.1.1"}],
           }
        }
    );

    let unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();

    // Test: a package that adds only possible requests
    let requests_checker = ImpossibleRequestsChecker::default();
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
            { "pkg": "new-pkg/2.0.0/3I42H3S6" },
            { "pkg": "new-pkg/2.0.0/src" },

            { "pkg": "my-pkg/2.0.0/3I42H3S6" },
            { "pkg": "my-pkg/1.0.0/3I42H3S6" },
            { "pkg": "my-pkg/1.0.0/src" },

            { "pkg": "my-pkg-b/3.0.0/3I42H3S6" },
            { "pkg": "my-pkg-b/3.1.1/3I42H3S6" },
            { "pkg": "my-pkg-b/3.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    // Thie second request is the impossible one. The IfAlreadyPresent
    // request is possible because it is just IfAlreadyPresent and has
    // not be requested by anything else yet.
    let spec = spec!(
        { "pkg": "about-to-resolve/1.0.0/3I42H3S6",
           "install": {
               "requirements": [{"pkg": "not-present/1.0.0",
                                 "include": "IfAlreadyPresent"},
                                {"pkg": "my-pkg/7.0.0"},
                                {"pkg": "my-pkg-b/3.1.1"},
                                {"var": "python.abi/3.9.7"}],
           }
        }
    );

    let unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();

    // Test: a package that adds an impossible request
    let requests_checker = ImpossibleRequestsChecker::default();
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
        { "pkg": "another-to-resolve/2.0.0/3I42H3S6",
           "install": {
               "requirements": [{"var": "python.abi/3.9.7"},
                                {"pkg": "my-pkg/7.0.0"},
                                {"pkg": "my-pkg-b/3.1.1"}],
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

    let request = PkgRequest::from_ident(
        version_ident!("my-pkg/2").to_any(None),
        RequestedBy::SpkInternalTest,
    );

    let mut unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();
    unresolved_requests.insert(request.pkg.name.clone(), request.clone());

    // Test: a package that has no install requirements of its own
    let requests_checker = ImpossibleRequestsChecker::default();
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
#[tokio::test]
async fn test_impossible_requests_checker_with_uncombinable_requests() {
    init_logging();

    let repo = make_repo!(
        [
            { "pkg": "my-pkg/1.0.0/3I42H3S6" },
            { "pkg": "my-pkg/1.0.0/src" },
        ]
    );
    let arc_repo = Arc::new(repo);

    let spec = spec!(
        { "pkg": "about-to-resolve/1.0.0/3I42H3S6",
           "install": {
               "requirements": [{"pkg": "my-pkg/21.0.0"}],
           }
        }
    );

    let request = PkgRequest::from_ident(
        version_ident!("my-pkg/1.0.0").to_any(None),
        RequestedBy::SpkInternalTest,
    );
    let mut unresolved_requests: HashMap<PkgNameBuf, PkgRequest> = HashMap::new();
    unresolved_requests.insert(request.pkg.name.clone(), request);

    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: an uncombinable request should return incompatible. This
    // exercises the possible requests caching for coverage.
    let compat = requests_checker
        .validate_pkg_requests(&spec, &unresolved_requests, &[Arc::clone(&arc_repo)])
        .await
        .unwrap();

    assert!(
        !compat.is_ok(),
        "There should make an impossible request due to an uncombinable request, but somehow hasn't"
    );
}

#[rstest]
fn test_impossible_requests_checker_reset() {
    init_logging();

    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: calling reset
    requests_checker.reset();

    assert!(
        requests_checker
            .num_ifalreadypresent_requests
            .load(Ordering::Relaxed)
            == 0
            && requests_checker
                .num_impossible_requests_found
                .load(Ordering::Relaxed)
                == 0
            && requests_checker
                .num_possible_requests_found
                .load(Ordering::Relaxed)
                == 0
            && requests_checker
                .num_impossible_cache_hits
                .load(Ordering::Relaxed)
                == 0
            && requests_checker
                .num_possible_cache_hits
                .load(Ordering::Relaxed)
                == 0
            && requests_checker
                .num_build_specs_read
                .load(Ordering::Relaxed)
                == 0
            && requests_checker
                .num_read_tasks_spawned
                .load(Ordering::Relaxed)
                == 0
            && requests_checker
                .num_read_tasks_stopped
                .load(Ordering::Relaxed)
                == 0,
        "Reset should have zeroed out the counters, but didn't"
    );
}

#[rstest]
fn test_impossible_requests_get_impossible_requests() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: getting impossible requests cache
    let requests = requests_checker.impossible_requests();

    assert!(requests.is_empty(), "Impossible requests should be empty");
}

#[rstest]
fn test_impossible_requests_get_possible_requests() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: getting possible requests cache
    let requests = requests_checker.possible_requests();

    assert!(requests.is_empty(), "Possible requests should be empty");
}

#[rstest]
fn test_impossible_requests_num_ifalreadypresent_requests() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of IfAlreadyPresent requests
    let number = requests_checker.num_ifalreadypresent_requests();

    assert!(number == 0, "IfAlreadyPresent counter should be zero");
}

#[rstest]
fn test_impossible_requests_get_impossible_requests_found() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of impossible requests
    let number = requests_checker.num_impossible_requests_found();

    assert!(number == 0, "Impossible requests counter should be zero");
}

#[rstest]
fn test_impossible_requests_num_possible_requests_found() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of possible requests
    let number = requests_checker.num_possible_requests_found();

    assert!(number == 0, "Possible requests counter should be zero");
}

#[rstest]
fn test_impossible_requests_num_impossible_hits() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of impossible cache hits requests
    let number = requests_checker.num_impossible_hits();

    assert!(number == 0, "Impossible cache hits counter should be zero");
}

#[rstest]
fn test_impossible_requests_num_possible_hits() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of possible cache hits requests
    let number = requests_checker.num_possible_hits();

    assert!(number == 0, "Possible cache hits counter should be zero");
}

#[rstest]
fn test_impossible_requests_build_specs_read() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of build specs read
    let number = requests_checker.num_build_specs_read();

    assert!(number == 0, "Builds specs read counter should be zero");
}

#[rstest]
fn test_impossible_requests_read_tasks_spawned() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of read tasks spawned
    let number = requests_checker.num_read_tasks_spawned();

    assert!(number == 0, "Read tasks spawned counter should be zero");
}

#[rstest]
fn test_impossible_requests_read_tasks_stopped() {
    let requests_checker = ImpossibleRequestsChecker::default();

    // Test: get number of read tasks stopped
    let number = requests_checker.num_read_tasks_stopped();

    assert!(number == 0, "Read tasks stopped counter should be zero");
}
