// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use spfs::tracking::Manifest;
use spk_schema::foundation::option_map;
use spk_schema::ident::PkgRequest;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{Package, ValidationRule, spec};
use spk_solve::{RequestedBy, Solution};

use crate::report::BuildSetupReport;
use crate::validation::Validator;

#[tokio::test]
async fn test_for_description_over_limit() {
    let description = "This is a test description.".repeat(14);
    let package = Arc::new(spec!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "sources": [],
            "build": {
                "options": [{"var": "inherited/val", "description": description}],
                "script": "echo building...",
            },
        }
    ));

    let mut environment = Solution::default();
    environment.add(
        PkgRequest::from_ident(package.ident().to_any_ident(), RequestedBy::DoesNotMatter),
        package.clone(),
        spk_solve::PackageSource::SpkInternalTest,
    );

    let setup = BuildSetupReport {
        environment,
        variant: option_map! {},
        environment_filesystem: Manifest::new(
            spfs::tracking::Entry::empty_dir_with_open_perms_with_data(package.ident().clone()),
        ),
        package,
    };

    ValidationRule::Deny {
        condition: ValidationMatcher::LongVarDescription,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("Should return error when description is over limit");
}
