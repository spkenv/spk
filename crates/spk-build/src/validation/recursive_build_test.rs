// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use rstest::rstest;
use spfs::tracking::Manifest;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::option_map;
use spk_schema::ident::PkgRequest;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{spec, Package, ValidationRule};
use spk_solve::{RequestedBy, Solution};

use crate::report::BuildSetupReport;
use crate::validation::Validator;

#[rstest]
#[tokio::test]
async fn test_build_with_circular_dependency() {
    init_logging();
    // The system should not allow a package to be built
    // that has a circular dependency.

    // Start out with a package with no dependencies.
    let old_build = Arc::new(spec!({
        "api": "v0/package",
        "pkg": "one/1.0.0/3TCOOP2W",
    }));

    let new_build = Arc::new(spec!({
        "api": "v0/package",
        "pkg": "one/2.0.0/3TCOOP2W",
    }));

    let mut environment = Solution::default();
    environment.add(
        PkgRequest::from_ident(old_build.ident().to_any(), RequestedBy::DoesNotMatter),
        old_build.clone(),
        spk_solve::PackageSource::SpkInternalTest,
    );

    let setup = BuildSetupReport {
        environment,
        variant: option_map! {},
        environment_filesystem: Manifest::new(
            spfs::tracking::Entry::empty_dir_with_open_perms_with_data(new_build.ident().clone()),
        ),
        package: new_build,
    };
    ValidationRule::Deny {
        condition: ValidationMatcher::RecursiveBuild,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("should get error when package appears in its own build environment");
}
