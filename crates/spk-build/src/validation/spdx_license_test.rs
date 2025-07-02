// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::sync::Arc;

use spfs::tracking::Manifest;
use spk_schema::foundation::option_map;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{Package, ValidationRule, spec};
use spk_solve::Solution;

use crate::report::BuildSetupReport;
use crate::validation::Validator;

macro_rules! basic_setup {
    ($pkg:tt) => {{
        let package = Arc::new(spec!($pkg));

        let environment = Solution::default();
        BuildSetupReport {
            environment,
            variant: option_map! {},
            environment_filesystem: Manifest::new(
                spfs::tracking::Entry::empty_dir_with_open_perms_with_data(package.ident().clone()),
            ),
            package,
        }
    }};
}

#[tokio::test]
async fn test_license_allowed_empty() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {},
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Allow {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect("Should allow no license with default allow rule");
}

#[tokio::test]
async fn test_license_allowed_valid() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {
                "license": "Apache-2.0" // from spdx license list
            },
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Allow {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect("Should allow a known license with default allow rule");
}

#[tokio::test]
async fn test_license_allowed_invalid() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {
                "license": "unknown" // NOT from spdx license list
            },
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Allow {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("Should fail with default allow rule and invalid license");
}

#[tokio::test]
async fn test_license_require_empty() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {},
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Require {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("Should fail when no license and require rule");
}

#[tokio::test]
async fn test_license_deny_empty() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {},
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Deny {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect("Should allow empty license with deny rule");
}

#[tokio::test]
async fn test_license_deny_invalid() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {
                "license": "unknown" // NOT from license list
            },
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Deny {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect("Should allow invalid license with deny rule");
}

#[tokio::test]
async fn test_license_deny_valid() {
    let setup = basic_setup!(
        {
            "pkg": "base/1.0.0/3TCOOP2W",
            "meta": {
                "license": "Apache-2.0"
            },
            "sources": [],
            "build": {
                "script": "echo building...",
            },
        }
    );

    ValidationRule::Deny {
        condition: ValidationMatcher::SpdxLicense,
    }
    .validate_setup(&setup)
    .await
    .into_result()
    .expect_err("Should fail with valid license and deny rule");
}
