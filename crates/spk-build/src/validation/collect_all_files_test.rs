// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spfs::tracking::Manifest;
use spk_schema::validation::ValidationMatcher;
use spk_schema::{v0, Package, ValidationRule};
use spk_solve::Solution;

use crate::report::{BuildOutputReport, BuildReport, BuildSetupReport, BuiltComponentReport};
use crate::validation::Validator;

#[tokio::test]
async fn test_validate_build_changeset_collected() {
    let mut package = v0::Spec::new("test-pkg/1.0.0/3I42H3S6".parse().unwrap());
    // the default components are added and collect all files,
    // so we remove them to ensure nothing is collected
    let _ = package.install.components.drain(..);
    let report = BuildReport {
        output: BuildOutputReport {
            collected_changes: vec![spfs::tracking::Diff {
                path: "/spfs/file.txt".into(),
                mode: spfs::tracking::DiffMode::Added(
                    spfs::tracking::Entry::empty_file_with_open_perms_with_data(
                        package.ident().clone(),
                    ),
                ),
            }],
            components: package
                .install
                .components
                .iter()
                .map(|c| {
                    (
                        c.name.clone(),
                        BuiltComponentReport {
                            layer: spfs::encoding::NULL_DIGEST.into(),
                            // notably, this manifest does not include the one collected
                            // file from above
                            manifest: spfs::tracking::Manifest::default(),
                        },
                    )
                })
                .collect(),
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
        condition: ValidationMatcher::CollectAllFiles,
    }
    .validate_build(&report)
    .await
    .into_result()
    .expect_err("should get error when a file is created that was not in a component spec");
}
