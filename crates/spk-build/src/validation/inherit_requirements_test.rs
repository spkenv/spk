// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::sync::Arc;

use spfs::tracking::Manifest;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::option_map;
use spk_schema::ident::PkgRequestWithOptions;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{Package, PinnableRequest, ValidationRule, spec};
use spk_solve::{RequestedBy, Solution};

use crate::report::BuildSetupReport;
use crate::validation::Validator;

#[tokio::test]
async fn test_build_package_downstream_build_requests() {
    init_logging();
    let base_spec = Arc::new(spec!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "sources": [],
            "build": {
                "options": [{"var": "inherited/val", "inheritance": "StrongForBuildOnly"}],
                "script": "echo building...",
            },
        }
    ));

    let package = spec!(
        {
            "pkg": "top/1.0.0/3TCOOP2W",
            "sources": [],
            "build": {"options": [{"pkg": "base"}], "script": "echo building..."},
        }
    );

    let mut environment = Solution::default();
    environment.add(
        PkgRequestWithOptions::from_ident(
            base_spec.ident().to_any_ident(),
            RequestedBy::DoesNotMatter,
        ),
        base_spec.clone(),
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
    let err = ValidationRule::Require {
        condition: ValidationMatcher::InheritRequirements {
            packages: Vec::new(),
        },
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("should get error when generated package is missing downstream requirement");

    let crate::Error::ValidationFailed { errors } = err else {
        panic!("Expected validation to fail, got {err:?}");
    };

    let err = errors.first().unwrap();

    match err {
        super::Error::DownstreamBuildRequestRequired {
            required_by,
            request,
            ..
        } => {
            assert_eq!(required_by, base_spec.ident());
            assert!(
                matches!(request, PinnableRequest::Var(v) if v.var.as_str() == "base.inherited" && v.value.as_pinned() == Some("val")),
                "{request}"
            );
        }
        _ => panic!("Expected Error::DownstreamBuildRequestRequired, got {err:?}"),
    }
}

#[tokio::test]
async fn test_build_package_downstream_runtime_request() {
    init_logging();
    let base_spec = Arc::new(spec!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "sources": [],
            "build": {
                "options": [{"var": "inherited/val", "inheritance": "Strong"}],
                "script": "echo building...",
            },
        }
    ));
    let package = spec!(
        {
            "pkg": "top/1.0.0/3TCOOP2W",
            "sources": [],
            "build": {"options": [{"pkg": "base"}, {"var": "inherited/val"}], "script": "echo building..."},
        }
    );

    let mut environment = Solution::default();
    environment.add(
        PkgRequestWithOptions::from_ident(
            base_spec.ident().to_any_ident(),
            RequestedBy::DoesNotMatter,
        ),
        base_spec.clone(),
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
    let err = ValidationRule::Require {
        condition: ValidationMatcher::InheritRequirements {
            packages: Vec::new(),
        },
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("should get error when generated package is missing downstream requirement");

    let crate::Error::ValidationFailed { errors } = err else {
        panic!("Expected validation to fail, got {err:?}");
    };

    let err = errors.first().unwrap();

    match err {
        super::Error::DownstreamRuntimeRequestRequired {
            required_by,
            request,
            ..
        } => {
            assert_eq!(required_by, base_spec.ident());
            assert!(
                matches!(&request, spk_schema::PinnableRequest::Var(v) if v.var.as_str() == "base.inherited" && v.value.as_pinned() == Some("val")),
                "{request}"
            );
        }
        _ => panic!("Expected Error::DownstreamRuntimeRequestRequired, got {err}"),
    }
}
