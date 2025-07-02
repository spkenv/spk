// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spfs::tracking::Manifest;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{Package, ValidationRule, v0};
use spk_solve::{Named, Solution};

use crate::report::{BuildOutputReport, BuildReport, BuildSetupReport};
use crate::validation::Validator;

#[tokio::test]
async fn test_validate_build_changeset_collect_existing() {
    let dependency_package = v0::Spec::new("dep-pkg/1.0.0/3I42H3S6".parse().unwrap());
    let package = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());

    let mut environment_filesystem = Manifest::new(
        spfs::tracking::Entry::empty_dir_with_open_perms_with_data(package.ident().clone()),
    );
    environment_filesystem
        .mknod(
            "/file.txt",
            spfs::tracking::Entry::empty_file_with_open_perms_with_data(
                dependency_package.ident().clone(),
            ),
        )
        .unwrap();
    let report = BuildReport {
        output: BuildOutputReport {
            collected_changes: vec![spfs::tracking::Diff {
                path: "/file.txt".into(),
                mode: spfs::tracking::DiffMode::Unchanged(
                    spfs::tracking::Entry::empty_file_with_open_perms_with_data(
                        // this change is specifically a file from the dep package
                        dependency_package.ident().clone(),
                    ),
                ),
            }],
            ..Default::default()
        },
        setup: BuildSetupReport {
            environment: Solution::default(),
            variant: package.build.variants.first().cloned().unwrap_or_default(),
            environment_filesystem,
            package,
        },
    };
    ValidationRule::Deny {
        condition: ValidationMatcher::CollectExistingFiles {
            packages: Vec::default(),
        },
    }
    .validate_build(&report)
    .await
    .into_result()
    .expect_err("should get error when a file is collected from another package");

    ValidationRule::Allow {
        condition: ValidationMatcher::CollectExistingFiles {
            packages: vec![dependency_package.name().to_owned().into()],
        },
    }
    .validate_build(&report)
    .await
    .into_result()
    .expect("should allow collecting files from named packages");
}
