// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;
use serde_json::json;
use spk_schema_foundation::fixtures::*;
use spk_schema_foundation::version::Compatibility;
use spk_schema_ident::Request;

use super::RequirementsList;

#[rstest]
fn test_deserialize_no_duplicates() {
    serde_yaml::from_str::<RequirementsList>("[{pkg: python}, {pkg: other}]")
        .expect("should succeed in a simple case");
    serde_yaml::from_str::<RequirementsList>("[{pkg: python}, {pkg: python}]")
        .expect_err("should fail to deserialize with the same package twice");
}

#[rstest]
#[case::simple_pkg(
    json!([
        {"pkg": "pkg-a"},
        {"pkg": "pkg-b"},
    ]),
    json!({"pkg": "pkg-a"})
)]
#[case::global_var(
    json!([
        {"var": "global/value"},
    ]),
    json!({"var": "global/value"})
)]
#[case::global_matches_namespaced(
    json!([
        {"var": "local/value"},
    ]),
    json!({"var": "pkg.local/value"})
)]
#[case::two_namespaced_vars(
    json!([
        {"var": "pkg.local/value"},
    ]),
    json!({"var": "pkg.local/value"})
)]
#[should_panic]
#[case::different_namespaces(
    json!([
        {"var": "ns1.var/value"},
    ]),
    json!({"var": "ns2.var/value"})
)]
#[should_panic]
#[case::separate_pkg(
    json!([
        {"pkg": "pkg-a"},
        {"pkg": "pkg-b"},
    ]),
    json!({"pkg": "pkg-c"})
)]
fn test_contains_request(#[case] requests: serde_json::Value, #[case] contains: serde_json::Value) {
    init_logging();

    let reqs: RequirementsList = serde_json::from_value(requests).unwrap();
    let contains: Request = serde_json::from_value(contains).unwrap();
    tracing::debug!("is {contains} contained within this? {reqs}");
    assert_eq!(reqs.contains_request(&contains), Compatibility::Compatible);
}
