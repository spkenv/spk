// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spk_schema::version::IncompatibleReason;

use super::prelude::*;
use crate::ValidatorT;

/// Validates that the var install requirements do not conflict with the existing options.
#[derive(Clone, Copy, Default)]
pub struct VarRequirementsValidator {}

impl ValidatorT for VarRequirementsValidator {
    fn validate_package<P: Package>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility> {
        let options = state.get_option_map();
        for request in spec.runtime_requirements().iter() {
            if let RequestWithOptions::Var(request) = request {
                for (name, value) in options.iter() {
                    // Determine whether this option is relevant to the var
                    // request. An option is relevant if it names the exact
                    // same var, or — for un-namespaced (global) requests —
                    // if it shares the same base name.
                    //
                    // A namespaced request such as `demo.samenameaspkg` is
                    // scoped to its package and must match exactly; it must
                    // not be satisfied or contradicted by an unrelated option
                    // that merely shares its base name, such as the version of
                    // a package named `samenameaspkg`. This mirrors the resolvo
                    // solver, which scopes namespaced var requests to their
                    // package and treats global vars as distinct.
                    let is_exact_match = *name == request.var;
                    let is_global_base_match = request.var.namespace().is_none()
                        && request.var.base_name() == name.base_name();
                    if !is_exact_match && !is_global_base_match {
                        continue;
                    }
                    if value.is_empty() {
                        // empty option values do not provide a valuable opinion on the resolve
                        continue;
                    }
                    if &*request.value != value.as_str() {
                        return Ok(Compatibility::Incompatible(
                            IncompatibleReason::VarRequirementMismatch {
                                var: request.var.clone(),
                                requested: request.value.to_string(),
                                name: name.clone(),
                                value: value.clone(),
                            },
                        ));
                    }
                }
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        _state: &State,
        _recipe: &R,
    ) -> crate::Result<Compatibility> {
        // the recipe cannot tell us what the
        // runtime requirements will be
        Ok(Compatibility::Compatible)
    }
}
