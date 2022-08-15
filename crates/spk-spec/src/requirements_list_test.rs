// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::RequirementsList;

#[rstest]
fn test_deserialize_no_duplicates() {
    serde_yaml::from_str::<RequirementsList>("[{pkg: python}, {pkg: other}]")
        .expect("should succeed in a simple case");
    serde_yaml::from_str::<RequirementsList>("[{pkg: python}, {pkg: python}]")
        .expect_err("should fail to deserialize with the same package twice");
}
