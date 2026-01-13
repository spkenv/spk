// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::BTreeMap;

use variantly::Variantly;

use crate::ident::{AnyIdent, PkgRequest, RangeIdent, RequestedBy, Satisfy};
use crate::name::OptNameBuf;
use crate::version::{Compatibility, IncompatibleReason, Version};

/// A package request can have associated option values.
///
/// When two requests are merged, their associated option values are merged too.
/// If only one of the requests specify a given option, the merged request
/// becomes a `Partial` request for that option. A package with a "required" var
/// cannot satisfy a `Partial` request for that var.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Variantly)]
pub enum PkgRequestOptionValue {
    /// Two requests with a `Complete` value merge into a `Complete` value
    Complete(String),
    /// Two requests with a `Partial` value or missing value merge into a
    /// `Partial` value
    Partial(String),
}

impl PkgRequestOptionValue {
    /// Get the underlying value string
    #[inline]
    pub fn value(&self) -> &str {
        match self {
            PkgRequestOptionValue::Complete(v) | PkgRequestOptionValue::Partial(v) => v,
        }
    }

    /// Change the option value to be partial
    #[inline]
    fn convert_to_partial(&mut self) -> &mut Self {
        match self {
            PkgRequestOptionValue::Complete(v) => {
                *self = PkgRequestOptionValue::Partial(std::mem::take(v));
            }
            PkgRequestOptionValue::Partial(_) => {
                // already partial, nothing to do
            }
        }
        self
    }
}

/// Like an [`crate::option_map::OptionMap`] but tracks if options are missing
/// from requests when requests are merged.
pub type PkgRequestOptions = BTreeMap<OptNameBuf, PkgRequestOptionValue>;

/// A package request along with associated options.
///
/// This structure represents a package request along with all the var requests
/// that are defined for the same package.
#[derive(Clone, Debug, Eq, Ord, Hash, PartialEq, PartialOrd)]
pub struct PkgRequestWithOptions {
    pub pkg_request: PkgRequest,
    pub options: PkgRequestOptions,
}

impl PkgRequestWithOptions {
    /// Construct a request for the range ident
    ///
    /// A range ident carries no options so the options map will be empty.
    pub fn new(pkg: RangeIdent, requester: RequestedBy) -> Self {
        Self {
            pkg_request: PkgRequest::new(pkg, requester),
            options: PkgRequestOptions::default(),
        }
    }

    /// Construct a new simple request for the identified package
    ///
    /// An ident carries no options so the options map will be empty.
    pub fn from_ident<I: Into<AnyIdent>>(pkg: I, requester: RequestedBy) -> Self {
        Self {
            pkg_request: PkgRequest::from_ident(pkg, requester),
            options: PkgRequestOptions::default(),
        }
    }

    /// Return true if the given item satisfies this request.
    pub fn is_satisfied_by<T>(&self, satisfy: &T) -> Compatibility
    where
        T: Satisfy<Self>,
    {
        satisfy.check_satisfies_request(self)
    }

    /// Return true if the given version number is applicable to this request.
    ///
    /// This is used a cheap preliminary way to prune package
    /// versions that are not going to satisfy the request without
    /// needing to load the whole package spec.
    #[inline]
    pub fn is_version_applicable(&self, version: &Version) -> Compatibility {
        self.pkg_request.is_version_applicable(version)
    }

    /// Reduce the scope of this request to the intersection with another.
    pub fn restrict(&mut self, other: &PkgRequestWithOptions) -> Compatibility {
        let compat = self.pkg_request.restrict(&other.pkg_request);
        if !compat.is_ok() {
            return compat;
        }
        // Any complete options in `self` that are missing from `other` must
        // become partial.
        for (key, value) in self.options.iter_mut() {
            if !other.options.contains_key(key) {
                value.convert_to_partial();
            }
        }
        // If both requests have the same option keys, they must have the same
        // values too.
        for (key, value) in other.options.iter() {
            match self.options.get(key) {
                Some(self_value) if self_value.value() != value.value() => {
                    return Compatibility::Incompatible(IncompatibleReason::VarOptionMismatch(
                        crate::version::VarOptionProblem::Incompatible {
                            assigned: self_value.value().to_string(),
                            value: value.value().to_string(),
                        },
                    ));
                }
                Some(PkgRequestOptionValue::Partial(_)) => {
                    // already partial, nothing to do
                }
                Some(PkgRequestOptionValue::Complete(_)) if value.is_complete() => {
                    // both are complete and equal, nothing to do
                }
                Some(_) => {
                    // our value is complete but the other is partial; make ours
                    // partial
                    self.options.insert(
                        key.clone(),
                        value.clone(), // must be partial here
                    );
                }
                None => {
                    // we need to remember that this option is only partially
                    // specified
                    self.options.insert(
                        key.clone(),
                        PkgRequestOptionValue::Partial(value.value().to_string()),
                    );
                }
            }
        }
        Compatibility::Compatible
    }
}

impl std::ops::Deref for PkgRequestWithOptions {
    type Target = PkgRequest;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.pkg_request
    }
}

impl std::ops::DerefMut for PkgRequestWithOptions {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pkg_request
    }
}
