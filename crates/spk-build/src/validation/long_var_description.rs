// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::validation::{
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Opt, Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::BuildSetupReport;

const MAX_LENGTH: usize = 256;

#[cfg(test)]
#[path = "./long_var_description_test.rs"]
mod long_var_description_test;

pub struct LongVarDescriptionValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for LongVarDescriptionValidator {}

#[async_trait::async_trait]
impl super::Validator for LongVarDescriptionValidator {
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
                        let mut outcome = Outcome {
                            condition: ValidationMatcherDiscriminants::LongVarDescription,
                            locality: v
                                .var
                                .with_default_namespace(setup.package.ident().name())
                                .to_string(),
                            subject: Subject::Package(setup.package.ident().clone()),
                            status: Status::Allowed,
                        };

                        match &self.kind {
                            RuleKind::Deny => {
                                if desc.chars().count() > MAX_LENGTH {
                                    outcome.status =
                                        Status::Denied(Error::LongVarDescriptionDenied);
                                };
                            }
                            RuleKind::Require => {
                                if desc.chars().count() <= MAX_LENGTH {
                                    outcome.status =
                                        Status::Denied(Error::LongVarDescriptionRequired);
                                };
                            }
                            RuleKind::Allow => (),
                        }
                        results.push(outcome);
                    }
                    None => continue,
                },
            }
        }
        Report::from_iter(results)
    }
}
