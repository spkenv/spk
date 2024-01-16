// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use spk_schema::validation::{
    ValidationMatcherDiscriminants,
    ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::{Inheritance, Opt, Package, Variant};

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::BuildSetupReport;

pub struct RequireDescriptionValidator {
    pub kind: RuleKind,
}

impl super::validator::sealed::Sealed for RequireDescriptionValidator {}

#[async_trait::async_trait]
impl super::Validator for RequireDescriptionValidator {
    async fn validate_setup<P, V>(&self, setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        let mut results = Vec::new();
        for opt in setup.package.get_build_options().iter() {
            match opt {
                Opt::Pkg(_) => continue,
                Opt::Var(v) => match v.inheritance {
                    Inheritance::Weak => continue,
                    _ => {
                        let mut outcome = Outcome {
                            condition: ValidationMatcherDiscriminants::RequireDescription,
                            locality: v
                                .var
                                .with_default_namespace(setup.package.ident().name())
                                .to_string(),
                            subject: Subject::Package(setup.package.ident().clone()),
                            status: Status::Allowed,
                        };

                        match &self.kind {
                            RuleKind::Deny => {
                                if v.description.is_some() {
                                    outcome.status = Status::Denied(Error::DescriptionNotRequired);
                                };
                            }
                            RuleKind::Require => {
                                if v.description.is_none() {
                                    outcome.status = Status::Denied(Error::NoDescription);
                                };
                            }
                            RuleKind::Allow => (),
                        }
                        results.push(outcome);
                    }
                },
            }
        }
        Report::from_iter(results)
    }
}
