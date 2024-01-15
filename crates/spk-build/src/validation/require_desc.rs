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
                Opt::Var(v) => {
                    let status = match v.inheritance {
                        Inheritance::Weak => continue,
                        _ => {
                            if v.description.is_some() {
                                Status::Allowed
                            } else {
                                Status::Denied(Error::NoDescription)
                            }
                        }
                    };

                    results.push(Outcome {
                        condition: ValidationMatcherDiscriminants::RequireDescription,
                        locality: String::new(),
                        subject: Subject::Package(setup.package.ident().clone()),
                        status,
                    });
                }
            }
        }
        Report::from_iter(results)
    }
}
