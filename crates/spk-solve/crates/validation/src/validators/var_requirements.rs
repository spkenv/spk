// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

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
            if let Request::Var(request) = request {
                for (name, value) in options.iter() {
                    let is_not_requested = *name != request.var;
                    let is_not_same_base = request.var.base_name() != name.base_name();
                    if is_not_requested && is_not_same_base {
                        continue;
                    }
                    if value.is_empty() {
                        // empty option values do not provide a valuable opinion on the resolve
                        continue;
                    }
                    let requested = request.value.as_pinned().unwrap_or_default();
                    if requested != value.as_str() {
                        return Ok(Compatibility::incompatible(format!(
                            "package wants {}={requested}, resolve has {name}={value}",
                            request.var
                        )));
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
