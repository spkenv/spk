// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::validation::{
    ValidationMatcherDiscriminants, ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::{BuildReport, BuildSetupReport};

#[cfg(test)]
#[path = "./empty_package_test.rs"]
mod empty_package_test;

pub struct EmptyPackageValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for EmptyPackageValidator {}

#[async_trait::async_trait]
impl super::Validator for EmptyPackageValidator {
    async fn validate_setup<P, V>(&self, _setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        Report::entire_build_not_matched(ValidationMatcherDiscriminants::EmptyPackage)
    }

    async fn validate_build<P, V>(&self, report: &BuildReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let is_empty = report.output.collected_changes.is_empty();
        let status = match self.kind {
            RuleKind::Deny if is_empty => Status::Denied(Error::EmptyPackageDenied),
            RuleKind::Require if !is_empty => Status::Required(Error::EmptyPackageRequired),
            _ => Status::Allowed,
        };
        Outcome {
            locality: String::new(),
            subject: Subject::Everything,
            status,
            condition: ValidationMatcherDiscriminants::EmptyPackage,
        }
        .into()
    }
}
