// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::validation::ValidationRuleDiscriminants as RuleKind;
use spk_schema::{Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::BuildSetupReport;

#[cfg(test)]
#[path = "./recursive_build_test.rs"]
mod recursive_build_test;

pub struct RecursiveBuildValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for RecursiveBuildValidator {}

#[async_trait::async_trait]
impl super::Validator for RecursiveBuildValidator {
    async fn validate_setup<P, V>(&self, setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let is_recursive = setup
            .environment
            .get(setup.package.name().as_str())
            .is_some();
        let status = match self.kind {
            RuleKind::Deny if is_recursive => {
                Status::Denied(Error::RecursiveBuildDenied(setup.package.name().to_owned()))
            }
            RuleKind::Require if !is_recursive => Status::Required(Error::RecursiveBuildRequired(
                setup.package.name().to_owned(),
            )),
            _ => Status::Allowed,
        };
        Outcome {
            locality: String::new(),
            subject: Subject::Everything,
            status,
            condition: spk_schema::validation::ValidationMatcherDiscriminants::RecursiveBuild,
        }
        .into()
    }
}
