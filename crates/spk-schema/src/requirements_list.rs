// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;
use std::fmt::Write;
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use spk_schema_foundation::IsDefault;
use spk_schema_foundation::name::{OptName, PkgName};
use spk_schema_foundation::spec_ops::Named;
use spk_schema_foundation::version::{Compatibility, IncompatibleReason};
use spk_schema_ident::{BuildIdent, PinPolicy};

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
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct RequirementsList<R = Request>(Vec<R>);

impl<R> Default for RequirementsList<R> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<R> IsDefault for RequirementsList<R> {
    fn is_default(&self) -> bool {
        self.is_empty()
    }
}

impl<R> std::ops::Deref for RequirementsList<R> {
    type Target = Vec<R>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<R> RequirementsList<R> {
    /// Remove all requests from this list
    pub fn clear(&mut self) {
        self.0.clear()
    }
}

impl<R> RequirementsList<R>
where
    R: Named<OptName>,
{
    /// Get a reference to the first request with the given name
    pub fn get(&self, name: impl AsRef<str>) -> Option<&R> {
        self.0.iter().find(|r| r.name() == name.as_ref())
    }

    /// Add or update a requirement in this list.
    ///
    /// If a request exists for the same name, it is replaced with the
    /// given one. Otherwise the new request is appended to the list.
    /// Returns the replaced request, if any.
    pub fn insert_or_replace(&mut self, request: R) -> Option<R> {
        let name = request.name();
        for existing in self.0.iter_mut() {
            if existing.name() == name {
                return Some(std::mem::replace(existing, request));
            }
        }
        self.0.push(request);
        None
    }

    /// Remove a requirement from this list.
    ///
    /// All requests with the same name as the given name are removed
    /// from the list. Typically, there can only be one entry for any
    /// given name, but this function does not rely on that assumption.
    pub fn remove_all<N>(&mut self, name: &N)
    where
        N: AsRef<OptName> + ?Sized,
    {
        self.0.retain(|existing| existing.name() != name.as_ref());
    }
}

impl RequirementsList<Request> {
    /// Add a requirement in this list, or merge it in.
    ///
    /// If a request exists for the same name, it is updated with the
    /// restrictions of this one. Otherwise the new request is
    /// appended to the list. Returns the newly inserted or updated request.
    pub fn insert_or_merge(&mut self, request: Request) -> Result<()> {
        let name = request.name();
        for existing in self.0.iter_mut() {
            if existing.name() != name {
                continue;
            }
            match (existing, &request) {
                (Request::Pkg(existing), Request::Pkg(request)) => {
                    if let incompatible @ Compatibility::Incompatible(_) =
                        existing.restrict(request)
                    {
                        return Err(Error::String(format!(
                            "Cannot insert requirement: {incompatible}"
                        )));
                    }
                }
                (existing, _) => {
                    return Err(Error::String(format!(
                        "Cannot insert requirement: one already exists and only pkg requests can be merged: {existing} + {request}"
                    )));
                }
            }
            return Ok(());
        }
        self.0.push(request);
        Ok(())
    }

    /// Reports whether the provided requests would be satisfied by
    /// this list of requests. The provided request does not need to
    /// exist in this list exactly, so long as there is a request in this
    /// list that is at least as restrictive
    pub fn contains_request(&self, theirs: &Request) -> Compatibility {
        let mut global_opt_request = None;
        for ours in self.iter() {
            match (ours, theirs) {
                (Request::Pkg(ours), Request::Pkg(theirs)) if ours.pkg.name == theirs.pkg.name => {
                    return ours.contains(theirs);
                }
                // a var request satisfy another if they have the same opt name or
                // if our request is package-less and has the same base name, eg:
                // name/value     [contains] name/value
                // pkg.name/value [contains] pkg.name/value
                // name/value     [contains] pkg.name/value
                //
                // We only exit early when we find a complete match. The last case
                // above is saved and only evaluated if no more specific request is found
                (Request::Var(ours), Request::Var(theirs)) if ours.var == theirs.var => {
                    return ours.contains(theirs);
                }
                (Request::Var(ours), Request::Var(theirs))
                    if theirs.var.namespace().is_some()
                        && ours.var.as_str() == theirs.var.base_name() =>
                {
                    global_opt_request = Some((ours, theirs));
                }
                _ => {
                    tracing::trace!("skip {ours}, not {theirs}");
                    continue;
                }
            }
        }
        if let Some((ours, theirs)) = global_opt_request {
            return ours.contains(theirs);
        }
        Compatibility::Incompatible(IncompatibleReason::RequirementsNotSuperset {
            name: theirs.name().to_owned(),
        })
    }

    /// Render all requests with a package pin using the given resolved packages.
    pub fn render_all_pins(
        &mut self,
        options: &OptionMap,
        resolved_by_name: &std::collections::HashMap<&PkgName, &BuildIdent>,
    ) -> Result<()> {
        self.0 = std::mem::take(&mut self.0).into_iter().filter_map(|request| {
            match &request {
                Request::Pkg(pkg_request) => {
                    match resolved_by_name.get(pkg_request.pkg.name()) {
                        None if pkg_request.pin.is_none() && pkg_request.pin_policy == PinPolicy::IfPresentInBuildEnv => {
                            None
                        }
                        _ if pkg_request.pin.is_none() => {
                            Some(Ok(request))
                        }
                        None if pkg_request.pin_policy == PinPolicy::IfPresentInBuildEnv => {
                            // This package was not in the build environment,
                            // but the pin policy allows this.
                            None
                        }
                        None => {
                            Some(Err(Error::String(
                                format!("Cannot resolve package using 'fromBuildEnv', package not present: {}\nIs it missing from your package build options?", pkg_request.pkg.name)
                            )))
                        }
                        Some(resolved) => {
                            Some(pkg_request.render_pin(resolved).map_err(Into::into).map(Request::Pkg))
                        }
                    }
                }
                Request::Var(var_request) => {
                    if !var_request.value.is_from_build_env() {
                        return Some(Ok(request));
                    }
                    let opts = match var_request.var.namespace() {
                        Some(ns) => options.package_options(ns),
                        None => options.clone(),
                    };
                    match opts.get(var_request.var.without_namespace()) {
                        None if var_request.value.is_from_build_env_if_present() => {
                            // This variable was not in the build environment,
                            // but the pin policy allows this.
                            None
                        }
                        None => {
                            Some(Err(Error::String(
                                format!("Cannot resolve variable using 'fromBuildEnv', variable not set: {}\nIs it missing from the package build options?", var_request.var)
                            )))
                        }
                        Some(opt) => {
                            Some(var_request.render_pin(opt.as_str()).map_err(Into::into).map(Request::Var))
                        }
                    }
                }
            }
        }).collect::<Result<Vec<_>>>()?;
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
}

impl<R> std::fmt::Display for RequirementsList<R>
where
    R: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_char('[')?;
        let mut entries = self.0.iter().peekable();
        while let Some(i) = entries.next() {
            i.fmt(f)?;
            if entries.peek().is_some() {
                f.write_str(", ")?;
            }
        }
        f.write_char(']')
    }
}

impl<R> IntoIterator for RequirementsList<R> {
    type Item = R;
    type IntoIter = std::vec::IntoIter<R>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'de, R> Deserialize<'de> for RequirementsList<R>
where
    R: serde::de::DeserializeOwned + Named<OptName>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RequirementsListVisitor<R>(PhantomData<fn() -> R>);

        impl<'de, R> serde::de::Visitor<'de> for RequirementsListVisitor<R>
        where
            R: serde::de::DeserializeOwned + Named<OptName>,
        {
            type Value = RequirementsList<R>;

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
                while let Some(request) = seq.next_element::<R>()? {
                    let name = request.name();
                    if !requirement_names.insert(name.to_owned()) {
                        return Err(serde::de::Error::custom(format!(
                            "found multiple install requirements for '{name}'"
                        )));
                    }
                    requirements.push(request);
                }
                Ok(RequirementsList(requirements))
            }
        }

        deserializer.deserialize_seq(RequirementsListVisitor(PhantomData))
    }
}
