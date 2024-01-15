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

const MAX_LENGTH: usize = 256;

pub struct LongDescriptionValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for LongDescriptionValidator {}

#[async_trait::async_trait]
impl super::Validator for LongDescriptionValidator {
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
                        match &self.kind {
                            RuleKind::Deny if desc.chars().count() > MAX_LENGTH => {
                                results.push(Outcome {
                                    condition: ValidationMatcherDiscriminants::LongDescription,
                                    locality: String::new(),
                                    subject: Subject::Package(setup.package.ident().clone()),
                                    status: Status::Denied(Error::DescriptionOverLimit),
                                });
                            }
                            RuleKind::Require if desc.chars().count() > MAX_LENGTH => {
                                results.push(Outcome {
                                    condition: ValidationMatcherDiscriminants::LongDescription,
                                    locality: String::new(),
                                    subject: Subject::Package(setup.package.ident().clone()),
                                    status: Status::Allowed,
                                });
                            }
                            _ => {
                                results.push(Outcome {
                                    condition: ValidationMatcherDiscriminants::LongDescription,
                                    locality: String::new(),
                                    subject: Subject::Package(setup.package.ident().clone()),
                                    status: Status::Allowed,
                                });
                            }
                        }
                    },
                    None => continue,
                }
            }
        }
        Report::from_iter(results)
    }
}
