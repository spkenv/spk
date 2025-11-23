// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use rstest::rstest;

use crate::v0::EmbeddedPackageSpec;

#[rstest]
fn test_spec_is_invalid_with_only_name() {
    serde_yaml::from_str::<EmbeddedPackageSpec>("{pkg: test-pkg}")
        .expect_err("package specs require a build id");
}
