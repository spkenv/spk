// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spfs::tracking::Manifest;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{v0, Package, ValidationRule};
use spk_solve::Solution;

use crate::report::{BuildReport, BuildSetupReport};
use crate::validation::Validator;

#[tokio::test]
async fn test_validate_build_changeset_nothing() {
    let package = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());
    let report = BuildReport {
        setup: BuildSetupReport {
            environment: Solution::default(),
            variant: package.build.variants.first().cloned().unwrap_or_default(),
            environment_filesystem: Manifest::new(
                spfs::tracking::Entry::empty_dir_with_open_perms_with_data(package.ident().clone()),
            ),
            package,
        },
        output: Default::default(),
    };
    ValidationRule::Deny {
        condition: ValidationMatcher::EmptyPackage,
    }
    .validate_build(&report)
    .await
    .into_result()
    .unwrap_err();
}
