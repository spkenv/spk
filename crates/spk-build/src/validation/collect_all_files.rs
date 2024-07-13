// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;

use spk_schema::validation::{
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::{BuildReport, BuildSetupReport};

#[cfg(test)]
#[path = "./collect_all_files_test.rs"]
mod collect_all_files_test;

pub struct CollectAllFilesValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for CollectAllFilesValidator {}

#[async_trait::async_trait]
impl super::Validator for CollectAllFilesValidator {
    async fn validate_setup<P, V>(&self, _setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        Report::entire_build_not_matched(ValidationMatcherDiscriminants::CollectAllFiles)
    }

    async fn validate_build<P, V>(&self, report: &BuildReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let collected: HashSet<_> = report
            .output
            .components
            .iter()
            .flat_map(|(_, c)| c.manifest.walk_abs("/"))
            .map(|n| n.path)
            .collect();
        let mut uncollected = report
            .output
            .collected_changes
            .iter()
            .filter(|diff| !collected.contains(&diff.path));
        match self.kind {
            RuleKind::Allow => {
                if uncollected.any(|_| true) {
                    Report::entire_build_allowed(ValidationMatcherDiscriminants::CollectAllFiles)
                } else {
                    Report::entire_build_not_matched(
                        ValidationMatcherDiscriminants::CollectAllFiles,
                    )
                }
            }
            RuleKind::Require => {
                if uncollected.any(|_| true) {
                    Outcome {
                        condition: ValidationMatcherDiscriminants::CollectAllFiles,
                        locality: String::new(),
                        subject: Subject::Package(report.setup.package.ident().clone()),
                        status: Status::Required(Error::CollectAllFilesDenied),
                    }
                    .into()
                } else {
                    Report::entire_build_not_matched(
                        ValidationMatcherDiscriminants::CollectAllFiles,
                    )
                }
            }
            RuleKind::Deny => uncollected
                .map(|diff| {
                    let status = Status::Required(Error::CollectAllFilesRequired {
                        path: diff.path.clone(),
                    });
                    let subject = Subject::Path(diff.mode.user_data().clone(), diff.path.clone());
                    Outcome {
                        subject,
                        condition: ValidationMatcherDiscriminants::CollectAllFiles,
                        locality: String::new(),
                        status,
                    }
                })
                .collect(),
        }
    }
}
