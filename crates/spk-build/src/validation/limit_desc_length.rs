// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::validation::{
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Opt, Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::BuildSetupReport;

pub struct LimitDescLengthValidator {
    pub kind: RuleKind,
    pub limit: usize,
}

impl super::validator::sealed::Sealed for LimitDescLengthValidator {}

#[async_trait::async_trait]
impl super::Validator for LimitDescLengthValidator {
    async fn validate_setup<P, V>(&self, setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let mut results = Vec::new();
        for opt in setup.package.get_build_options().iter() {
            match opt {
                Opt::Pkg(_) => continue,
                Opt::Var(v) => match &v.description {
                    Some(desc) => {
                        let status = if desc.chars().count() <= self.limit {
                            Status::Allowed
                        } else {
                            Status::Denied(Error::DescriptionOverLimit { limit: self.limit })
                        };

                        results.push(Outcome {
                            condition: ValidationMatcherDiscriminants::LimitDescLength,
                            locality: String::new(),
                            subject: Subject::Package(setup.package.ident().clone()),
                            status,
                        });
                    }
                    None => continue,
                },
            }
        }
        Report::from_iter(results)
    }
}
