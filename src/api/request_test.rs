// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use serde_yaml;

use super::{InclusionPolicy, PkgRequest, PreReleasePolicy, VarRequest};

#[rstest]
fn test_prerelease_policy() {
    let mut a: PkgRequest =
        serde_yaml::from_str("{pkg: something, prereleasePolicy: IncludeAll}").unwrap();
    let b: PkgRequest =
        serde_yaml::from_str("{pkg: something, prereleasePolicy: ExcludeAll}").unwrap();

    a.restrict(&b).unwrap();
    match a.prerelease_policy {
        PreReleasePolicy::ExcludeAll => (),
        _ => panic!("expected restricted prerelease policy"),
    }
}

#[rstest]
fn test_inclusion_policy() {
    let mut a: PkgRequest =
        serde_yaml::from_str("{pkg: something, include: IfAlreadyPresent}").unwrap();
    let b: PkgRequest = serde_yaml::from_str("{pkg: something, include: Always}").unwrap();

    a.restrict(&b).unwrap();
    match a.inclusion_policy {
        InclusionPolicy::Always => (),
        _ => panic!("expected restricted inclusion policy"),
    }
}

#[rstest]
fn test_deserialize_value_or_pin() {
    let res = serde_yaml::from_str::<VarRequest>("{var: python.abi/cp27m}");
    assert!(res.is_ok(), "should allow regular name/value");

    let res = serde_yaml::from_str::<VarRequest>("{var: python.abi, fromBuildEnv: true}");
    assert!(res.is_ok(), "should allow no value when pinning build env");

    let res = serde_yaml::from_str::<VarRequest>("{var: python.abi/cp27m, fromBuildEnv: true}");
    assert!(res.is_err(), "should not allow value and pin");

    let res = serde_yaml::from_str::<VarRequest>("{var: python.abi}");
    assert!(res.is_err(), "should not allow omitting value without pin");
}

#[rstest]
fn test_var_request_empty_value_roundtrip() {
    let req = serde_yaml::from_str::<VarRequest>("{var: python.abi/}").unwrap();
    let yaml = serde_yaml::to_string(&req).unwrap();
    let res = serde_yaml::from_str::<VarRequest>(&yaml);
    assert!(
        res.is_ok(),
        "should be able to round-trip serialize a var request with empty string value"
    );
}

#[rstest]
fn test_var_request_pinned_roundtrip() {
    let req = serde_yaml::from_str::<VarRequest>("{var: python.abi, fromBuildEnv: true}").unwrap();
    let yaml = serde_yaml::to_string(&req).unwrap();
    let res = serde_yaml::from_str::<VarRequest>(&yaml);
    assert!(
        res.is_ok(),
        "should be able to round-trip serialize a var request with pin"
    );
    assert!(res.unwrap().pin, "should preserve pin value");
}
