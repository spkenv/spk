// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::version::{IncompatibleReason, PackageRepoProblem};

use super::prelude::*;
use crate::ValidatorT;

/// Ensures that a package meets all requested version criteria.
#[derive(Clone, Copy)]
pub struct PkgRequestValidator {}

impl ValidatorT for PkgRequestValidator {
    #[allow(clippy::nonminimal_bool)]
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Package,
    {
        self.validate_package_against_request(state, spec, source)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        let request = match state.get_merged_request(recipe.name()) {
            Ok(request) => request,
            Err(GetMergedRequestError::NoRequestFor(name)) => {
                return Ok(Compatibility::Incompatible(
                    IncompatibleReason::InternalError(format!(
                        "package '{name}' was not requested"
                    )),
                ));
            }
            Err(err) => {
                return Ok(Compatibility::Incompatible(
                    IncompatibleReason::InternalError(format!(
                        "package '{name}' has an invalid request stack: {err}",
                        name = recipe.name()
                    )),
                ));
            }
        };
        Ok(request.is_version_applicable(recipe.version()))
    }

    #[allow(clippy::nonminimal_bool)]
    fn validate_package_against_request<PR, P>(
        &self,
        pkgrequest_data: &PR,
        package: &P,
        source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Satisfy<PkgRequest> + Package,
        PR: GetMergedRequest,
    {
        let request = match pkgrequest_data.get_merged_request(package.name()) {
            Ok(request) => request,
            Err(GetMergedRequestError::NoRequestFor(name)) => {
                return Ok(Compatibility::Incompatible(
                    IncompatibleReason::InternalError(format!(
                        "package '{name}' was not requested"
                    )),
                ));
            }
            Err(err) => {
                return Ok(Compatibility::Incompatible(
                    IncompatibleReason::InternalError(format!(
                        "package '{name}' has an invalid request stack: {err}",
                        name = package.name()
                    )),
                ));
            }
        };

        if let Some(rn) = &request.pkg.repository_name {
            // If the request names a repository, then the source has to match.
            match source {
                PackageSource::Repository { repo, .. } if repo.name() != rn => {
                    return Ok(Compatibility::Incompatible(
                        IncompatibleReason::PackageRepoMismatch(
                            PackageRepoProblem::WrongSourceRepository {
                                self_repo: rn.clone(),
                                their_repo: repo.name().to_owned(),
                            },
                        ),
                    ));
                }
                PackageSource::Repository { .. } => {} // okay
                PackageSource::Embedded { parent, .. } => {
                    // TODO: from the right repo still?
                    return Ok(Compatibility::Incompatible(
                        IncompatibleReason::PackageRepoMismatch(
                            PackageRepoProblem::EmbeddedInPackageFromWrongRepository {
                                parent_ident: parent.to_string(),
                            },
                        ),
                    ));
                }
                PackageSource::BuildFromSource { .. } => {
                    // TODO: from the right repo still?
                    return Ok(Compatibility::Incompatible(
                        IncompatibleReason::PackageRepoMismatch(
                            PackageRepoProblem::FromRecipeFromWrongRepository,
                        ),
                    ));
                }
                PackageSource::SpkInternalTest => {
                    return Ok(Compatibility::Incompatible(
                        IncompatibleReason::PackageRepoMismatch(PackageRepoProblem::InternalTest),
                    ));
                }
            };
        }
        // the initial check is more general and provides more user
        // friendly error messages that we'd like to get
        let mut compat = request.is_version_applicable(package.version());
        if !!&compat {
            compat = request.is_satisfied_by(package)
        }
        Ok(compat)
    }
}
