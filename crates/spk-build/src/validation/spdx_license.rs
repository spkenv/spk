// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::validation::{
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::{BuildReport, BuildSetupReport};

#[cfg(test)]
#[path = "./spdx_license_test.rs"]
mod spdx_license_test;

pub struct SpdxLicenseValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for SpdxLicenseValidator {}

#[async_trait::async_trait]
impl super::Validator for SpdxLicenseValidator {
    async fn validate_setup<P, V>(&self, setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let meta = setup.package.metadata();
        let exists = meta.license.is_some();
        let is_valid = match meta.license.as_ref() {
            Some(value) => spdx::license_id(value).is_some(),
            None => true,
        };
        let status = match self.kind {
            RuleKind::Require if !exists => Status::Required(Error::SpdxLicenseMissing),
            RuleKind::Allow | RuleKind::Require if !is_valid => {
                Status::Required(Error::SpdxLicenseInvalid {
                    given: meta.license.clone().unwrap_or_default(),
                })
            }
            RuleKind::Deny if exists && is_valid => Status::Denied(Error::SpdxLicenseDenied),
            _ => Status::Allowed,
        };
        Outcome {
            locality: String::new(),
            subject: Subject::Everything,
            status,
            condition: ValidationMatcherDiscriminants::SpdxLicense,
        }
        .into()
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
