// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use itertools::Itertools;
use spk_schema::validation::{
    FileAlteration,
    NameOrCurrent,
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::{BuildReport, BuildSetupReport};

#[cfg(test)]
#[path = "./alter_existing_files_test.rs"]
mod alter_existing_files_test;

pub struct AlterExistingFilesValidator<'a> {
    pub kind: RuleKind,
    pub packages: &'a Vec<NameOrCurrent>,
    pub action: Option<&'a FileAlteration>,
}

impl<'a> super::validator::sealed::Sealed for AlterExistingFilesValidator<'a> {}

#[async_trait::async_trait]
impl<'a> super::Validator for AlterExistingFilesValidator<'a> {
    async fn validate_setup<P, V>(&self, _setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        Report::entire_build_not_matched(ValidationMatcherDiscriminants::AlterExistingFiles)
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
        let action = self.action.copied().unwrap_or_default();
        let mut altered_files =
            report
                .output
                .collected_changes
                .iter()
                .filter(|diff| match (action, &diff.mode) {
                    _ if diff.mode.is_dir() => false,
                    (FileAlteration::Remove, spfs::tracking::DiffMode::Removed(src))
                    | (FileAlteration::Touch, spfs::tracking::DiffMode::Unchanged(src))
                    | (FileAlteration::Change, spfs::tracking::DiffMode::Changed(src, _)) => {
                        names.is_empty() || names.contains(&src.user_data.name())
                    }
                    _ => false,
                });
        match self.kind {
            RuleKind::Allow => {
                let localities = names.iter().map(|n| format!("{action:?}/{n}"));
                if altered_files.any(|_| true) {
                    Report::entire_build_allowed_at(
                        ValidationMatcherDiscriminants::AlterExistingFiles,
                        localities,
                    )
                } else {
                    Report::entire_build_not_matched_at(
                        ValidationMatcherDiscriminants::AlterExistingFiles,
                        localities,
                    )
                }
            }
            RuleKind::Require => {
                if altered_files.any(|_| true) {
                    return Report::entire_build_allowed(
                        ValidationMatcherDiscriminants::AlterExistingFiles,
                    );
                }
                let owner = if names.is_empty() {
                    String::from("any package")
                } else {
                    names.iter().join(", or ")
                };
                let status = Status::Required(Error::AlterExistingFilesRequired { owner });
                Report::by_localities(names.iter().map(|n| n.to_string()), |locality| Outcome {
                    condition: ValidationMatcherDiscriminants::AlterExistingFiles,
                    locality: format!("{action:?}/{locality}"),
                    status: status.clone(),
                    subject: Subject::Everything,
                })
            }
            RuleKind::Deny => {
                altered_files
                    .map(|diff| {
                        let package = diff.mode.user_data().clone();
                        let local_package = self
                            .packages
                            .iter()
                            .filter_map(|p| p.as_name())
                            // if the matched subject is in the list of packages
                            // then that package is the locality of this rule
                            .find(|n| package.name() == *n)
                            .map(|n| n.as_str())
                            .unwrap_or_default();
                        let locality = format!("{action:?}/{local_package}");
                        let verb = match action {
                            FileAlteration::Change => "changed",
                            FileAlteration::Remove => "removed",
                            FileAlteration::Touch => "touched",
                        };
                        let status = Status::Denied(Error::AlterExistingFilesDenied {
                            owner: package.clone(),
                            path: diff.path.clone(),
                            action: verb,
                        });
                        let subject = Subject::Path(package, diff.path.clone());
                        Outcome {
                            condition: ValidationMatcherDiscriminants::AlterExistingFiles,
                            subject,
                            locality,
                            status,
                        }
                    })
                    .collect()
            }
        }
    }
}
