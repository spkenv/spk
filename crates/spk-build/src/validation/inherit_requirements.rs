// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::name::PkgNameBuf;
use spk_schema::validation::{
    ValidationMatcherDiscriminants, ValidationRuleDiscriminants as RuleKind,
};
use spk_schema::version::Compatibility;
use spk_schema::{Package, Variant};
use spk_solve::Named;

use super::{Error, Outcome, Report, Status, Subject};
use crate::report::BuildSetupReport;

#[cfg(test)]
#[path = "./inherit_requirements_test.rs"]
mod inherit_requirements_test;

pub struct InheritRequirementsValidator<'a> {
    pub kind: RuleKind,
    pub packages: &'a Vec<PkgNameBuf>,
}

impl super::validator::sealed::Sealed for InheritRequirementsValidator<'_> {}

#[async_trait::async_trait]
impl super::Validator for InheritRequirementsValidator<'_> {
    async fn validate_setup<P, V>(&self, setup: &BuildSetupReport<P, V>) -> Report
    where
        P: Package,
        V: Variant + Send + Sync,
    {
        // TODO: why can the below function fail? what errors might we hide here?
        let build_requirements = setup.package.get_build_requirements().unwrap_or_default();
        let runtime_requirements = setup.package.runtime_requirements();
        let mut results = Vec::new();
        for solved_request in setup.environment.items() {
            if !self.packages.is_empty()
                && !self
                    .packages
                    .iter()
                    .any(|n| n == solved_request.spec.name())
            {
                continue;
            }
            let locality = || {
                if self.packages.is_empty() {
                    solved_request.spec.name().to_string()
                } else {
                    Default::default()
                }
            };
            let all_components = solved_request.selected_components();
            for component in all_components {
                let downstream_build = solved_request
                    .spec
                    .downstream_build_requirements([component]);
                for request in downstream_build.iter() {
                    let compat = build_requirements.contains_request(request);
                    let status = match (self.kind, compat) {
                        (RuleKind::Allow, Compatibility::Compatible)
                        | (RuleKind::Allow, Compatibility::Incompatible(_))
                        | (RuleKind::Require, Compatibility::Compatible)
                        | (RuleKind::Deny, Compatibility::Incompatible(_)) => Status::Allowed,
                        (RuleKind::Require, Compatibility::Incompatible(reason)) => {
                            Status::Required(Error::DownstreamBuildRequestRequired {
                                required_by: solved_request.spec.ident().to_owned(),
                                request: request.clone(),
                                problem: reason.to_string(),
                            })
                        }
                        (RuleKind::Deny, Compatibility::Compatible) => {
                            Status::Denied(Error::DownstreamBuildRequestDenied {
                                request: request.clone(),
                            })
                        }
                    };
                    results.push(Outcome {
                        condition: ValidationMatcherDiscriminants::InheritRequirements,
                        locality: locality(),
                        subject: Subject::Package(setup.package.ident().clone()),
                        status,
                    })
                }
                let downstream_runtime = solved_request
                    .spec
                    .downstream_runtime_requirements([component]);
                for request in downstream_runtime.iter() {
                    let status = match (self.kind, runtime_requirements.contains_request(request)) {
                        (RuleKind::Allow, Compatibility::Compatible)
                        | (RuleKind::Allow, Compatibility::Incompatible(_))
                        | (RuleKind::Require, Compatibility::Compatible)
                        | (RuleKind::Deny, Compatibility::Incompatible(_)) => Status::Allowed,
                        (RuleKind::Require, Compatibility::Incompatible(reason)) => {
                            Status::Required(Error::DownstreamRuntimeRequestRequired {
                                required_by: solved_request.spec.ident().to_owned(),
                                request: request.clone(),
                                problem: reason.to_string(),
                            })
                        }
                        (RuleKind::Deny, Compatibility::Compatible) => {
                            Status::Denied(Error::DownstreamRuntimeRequestDenied {
                                request: request.clone(),
                            })
                        }
                    };
                    results.push(Outcome {
                        condition: ValidationMatcherDiscriminants::InheritRequirements,
                        locality: locality(),
                        subject: Subject::Package(setup.package.ident().clone()),
                        status,
                    });
                }
            }
        }
        Report::from_iter(results)
    }
}
