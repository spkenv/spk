// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs::tracking::Manifest;
use spk_schema::validation::{FileAlteration, ValidationMatcher};
use spk_schema::{v0, Package, ValidationRule};
use spk_solve::Solution;

use crate::report::{BuildOutputReport, BuildReport, BuildSetupReport};
use crate::validation::Validator;

#[tokio::test]
async fn test_validate_build_changeset_modified() {
    let package = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());
    let report = BuildReport {
        output: BuildOutputReport {
            collected_changes: vec![spfs::tracking::Diff {
                path: "/spfs/file.txt".into(),
                mode: spfs::tracking::DiffMode::Changed(
                    spfs::tracking::Entry::empty_file_with_open_perms_with_data(
                        "external/1.0.0/3I42H3S6".parse().unwrap(),
                    ),
                    spfs::tracking::Entry::empty_file_with_open_perms_with_data(
                        package.ident().clone(),
                    ),
                ),
            }],
            ..Default::default()
        },
        setup: BuildSetupReport {
            environment: Solution::default(),
            variant: package.build.variants[0].clone(),
            environment_filesystem: Manifest::new(
                spfs::tracking::Entry::empty_dir_with_open_perms_with_data(package.ident().clone()),
            ),
            package,
        },
    };
    ValidationRule::Deny {
        condition: ValidationMatcher::AlterExistingFiles {
            packages: Vec::new(),
            action: Some(FileAlteration::Change),
        },
    }
    .validate_build(&report)
    .await
    .into_result()
    .unwrap_err();
}
