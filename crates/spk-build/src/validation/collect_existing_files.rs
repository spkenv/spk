// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use itertools::Itertools;
use spk_schema::validation::{
    NameOrCurrent,
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::{BuildReport, BuildSetupReport};

#[cfg(test)]
#[path = "./collect_existing_files_test.rs"]
mod collect_existing_files_test;

pub struct CollectExistingFilesValidator<'a> {
    pub kind: RuleKind,
    pub packages: &'a Vec<NameOrCurrent>,
}

impl<'a> super::validator::sealed::Sealed for CollectExistingFilesValidator<'a> {}

#[async_trait::async_trait]
impl<'a> super::Validator for CollectExistingFilesValidator<'a> {
    async fn validate_setup<P, V>(&self, _setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        Report::entire_build_not_matched(ValidationMatcherDiscriminants::CollectExistingFiles)
    }

    async fn validate_build<P, V>(&self, report: &BuildReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let names: Vec<_> = self
            .packages
            .iter()
            .map(|n| n.or_current(report.setup.package.name()))
            .collect();
        let mut collected_env_files = report.output.collected_changes.iter().filter(|diff| {
            let Some(existing) = report.setup.environment_filesystem.get_path(&diff.path) else {
                return false;
            };
            if existing.is_dir() {
                return false;
            }
            names.is_empty() || names.contains(&existing.user_data.name())
        });
        match self.kind {
            RuleKind::Allow => {
                let localities = names.iter().map(|n| n.to_string());
                if collected_env_files.any(|_| true) {
                    Report::entire_build_allowed_at(
                        ValidationMatcherDiscriminants::CollectExistingFiles,
                        localities,
                    )
                } else {
                    Report::entire_build_not_matched_at(
                        ValidationMatcherDiscriminants::CollectExistingFiles,
                        localities,
                    )
                }
            }
            RuleKind::Require => {
                if collected_env_files.any(|_| true) {
                    Report::entire_build_allowed_at(
                        ValidationMatcherDiscriminants::CollectExistingFiles,
                        names.iter().map(|n| n.to_string()),
                    )
                } else {
                    let owner = if names.is_empty() {
                        String::from("any package")
                    } else {
                        names.iter().join(", or ")
                    };
                    Report::by_localities(names.iter().map(|n| n.to_string()), |locality| Outcome {
                        condition: ValidationMatcherDiscriminants::CollectExistingFiles,
                        status: Status::Denied(Error::CollectExistingFilesRequired {
                            owner: owner.clone(),
                        }),
                        subject: Subject::Everything,
                        locality,
                    })
                }
            }
            RuleKind::Deny => collected_env_files
                .map(|diff| {
                    let package = diff.mode.user_data().clone();
                    let locality = self
                        .packages
                        .iter()
                        .filter_map(|p| p.as_name())
                        // if the matched subject is in the list of packages (should always be the case)
                        // then that package is the locality of this rule. Ie this matched rule is considered
                        // more specific and more important than one that didn't name specific packages
                        .find(|n| package.name() == *n)
                        .map(|n| n.to_string())
                        .unwrap_or_default();
                    let status = Status::Denied(Error::CollectExistingFilesDenied {
                        owner: package.clone(),
                        path: diff.path.clone(),
                    });
                    let subject = Subject::Path(package, diff.path.clone());
                    Outcome {
                        condition: ValidationMatcherDiscriminants::CollectExistingFiles,
                        subject,
                        locality,
                        status,
                    }
                })
                .collect(),
        }
    }
}
