// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::version::Compatibility;
use spk_schema_ident::BuildIdent;

use crate::foundation::option_map::OptionMap;
use crate::ident::Request;
use crate::{Error, Result};

#[cfg(test)]
#[path = "./requirements_list_test.rs"]
mod requirements_list_test;

/// A set of installation requirements.
///
/// Requirements lists cannot contain multiple requests with the
/// same name, requiring instead that they be combined into a single
/// request as needed.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RequirementsList(Vec<Request>);

impl std::ops::Deref for RequirementsList {
    type Target = Vec<Request>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl RequirementsList {
    /// Add or update a requirement in this list.
    ///
    /// If a request exists for the same name, it is replaced with the
    /// given one. Otherwise the new request is appended to the list.
    /// Returns the replaced request, if any.
    pub fn insert_or_replace(&mut self, request: Request) -> Option<Request> {
        let name = request.name();
        for existing in self.0.iter_mut() {
            if existing.name() == name {
                return Some(std::mem::replace(existing, request));
            }
        }
        self.0.push(request);
        None
    }

    /// Add a requirement in this list, or merge it in.
    ///
    /// If a request exists for the same name, it is updated with the
    /// restrictions of this one one. Otherwise the new request is
    /// appended to the list. Returns the newly inserted or updated request.
    pub fn insert_or_merge(&mut self, request: Request) -> Result<()> {
        let name = request.name();
        for existing in self.0.iter_mut() {
            if existing.name() != name {
                continue;
            }
            match (existing, &request) {
                (Request::Pkg(existing), Request::Pkg(request)) => {
                    existing.restrict(request)?;
                }
                (existing, _) => {
                    return Err(Error::String(format!("Cannot insert requirement: one already exists and only pkg requests can be merged: {existing} + {request}")))
                }
            }
            return Ok(());
        }
        self.0.push(request);
        Ok(())
    }

    /// Reports whether the provided request would be satisfied by
    /// this list of requests. The provided request does not need to
    /// exist in this list exactly, so long as there is a request in this
    /// list that is at least as restrictive
    pub fn contains_request(&self, theirs: &Request) -> Compatibility {
        for ours in self.iter() {
            match (ours, theirs) {
                (Request::Pkg(ours), Request::Pkg(theirs)) => {
                    return ours.contains(theirs);
                }
                (Request::Var(ours), Request::Var(theirs)) => {
                    return ours.contains(theirs);
                }
                _ => continue,
            }
        }
        Compatibility::incompatible("No request exists for this")
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins<'a>(
        &mut self,
        options: &OptionMap,
        resolved: impl Iterator<Item = &'a BuildIdent>,
    ) -> Result<()> {
        let mut by_name = std::collections::HashMap::new();
        for pkg in resolved {
            by_name.insert(pkg.name(), pkg);
        }
        for request in self.0.iter_mut() {
            match request {
                Request::Pkg(request) => {
                    if request.pin.is_none() {
                        continue;
                    }
                    match by_name.get(request.pkg.name()) {
                        None => {
                            return Err(Error::String(
                                format!("Cannot resolve package using 'fromBuildEnv', package not present: {}\nIs it missing from your package build options?", request.pkg.name)
                            ));
                        }
                        Some(resolved) => {
                            let rendered = request.render_pin(resolved)?;
                            let _ = std::mem::replace(request, rendered);
                        }
                    }
                }
                Request::Var(request) => {
                    if !request.value.is_from_build_env() {
                        continue;
                    }
                    let opts = match request.var.namespace() {
                        Some(ns) => options.package_options(ns),
                        None => options.clone(),
                    };
                    match opts.get(request.var.without_namespace()) {
                        None => {
                            return Err(Error::String(
                                format!("Cannot resolve variable using 'fromBuildEnv', variable not set: {}\nIs it missing from the package build options?", request.var)
                            ));
                        }
                        Some(opt) => {
                            let rendered = request.render_pin(opt)?;
                            let _ = std::mem::replace(request, rendered);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Attempt to build a requirements list from a set of requests.
    ///
    /// Duplicate requests will be merged. Any error during this process
    /// will cause this process to fail.
    pub fn try_from_iter<I>(value: I) -> Result<Self>
    where
        I: IntoIterator<Item = Request>,
    {
        let mut out = Self::default();
        for item in value.into_iter() {
            out.insert_or_merge(item)?;
        }
        Ok(out)
    }

    /// Remove all requests from this list
    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl IntoIterator for RequirementsList {
    type Item = Request;
    type IntoIter = std::vec::IntoIter<Request>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'de> Deserialize<'de> for RequirementsList {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RequirementsListVisitor;

        impl<'de> serde::de::Visitor<'de> for RequirementsListVisitor {
            type Value = RequirementsList;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a list of requirements")
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let size_hint = seq.size_hint().unwrap_or(0);
                let mut requirements = Vec::with_capacity(size_hint);
                let mut requirement_names = HashSet::with_capacity(size_hint);
                while let Some(request) = seq.next_element::<Request>()? {
                    let name = request.name();
                    if requirement_names.contains(name) {
                        return Err(serde::de::Error::custom(format!(
                            "found multiple install requirements for '{name}'"
                        )));
                    }
                    requirement_names.insert(name.to_owned());
                    requirements.push(request);
                }
                Ok(RequirementsList(requirements))
            }
        }

        deserializer.deserialize_seq(RequirementsListVisitor)
    }
}
