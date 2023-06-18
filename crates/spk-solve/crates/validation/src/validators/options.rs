// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use super::prelude::*;
use crate::ValidatorT;

/// Ensures that a package is compatible with all requested options.
#[derive(Clone, Copy, Default)]
pub struct OptionsValidator {}

impl ValidatorT for OptionsValidator {
    fn validate_package<P>(
        &self,
        state: &State,
        spec: &P,
        _source: &PackageSource,
    ) -> crate::Result<Compatibility>
    where
        P: Package + Satisfy<VarRequest>,
    {
        let requests = state.get_var_requests();
        let qualified_requests: HashSet<_> = requests
            .iter()
            .filter_map(|r| {
                if r.var.namespace() == Some(spec.name()) {
                    Some(r.var.without_namespace())
                } else {
                    None
                }
            })
            .collect();
        for request in requests {
            if request.var.namespace().is_none() && qualified_requests.contains(&*request.var) {
                // a qualified request was found that supersedes this one:
                // eg: this is 'debug', but we have 'thispackage.debug'
                continue;
            }
            let compat = request.is_satisfied_by(spec);
            if !&compat {
                return Ok(Compatibility::incompatible(format!(
                    "doesn't satisfy requested option: {compat}"
                )));
            }
        }
        Ok(Compatibility::Compatible)
    }

    fn validate_recipe<R: Recipe>(
        &self,
        state: &State,
        recipe: &R,
    ) -> crate::Result<Compatibility> {
        if let Err(err) = recipe.resolve_options(state.get_option_map()) {
            Ok(Compatibility::incompatible(err.to_string()))
        } else {
            Ok(Compatibility::Compatible)
        }
    }
}
