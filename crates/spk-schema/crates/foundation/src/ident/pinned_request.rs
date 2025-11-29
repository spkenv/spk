// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::fmt::Write;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use variantly::Variantly;

use crate::ident::{
    InclusionPolicy,
    NameAndValue,
    PinPolicy,
    PinValue,
    PkgRequest,
    PreReleasePolicy,
    RangeIdent,
    VarRequest,
};
use crate::name::{OptName, OptNameBuf};
use crate::option_map::Stringified;

#[cfg(test)]
#[path = "./pinned_request_test.rs"]
mod pinned_request_test;

/// A pinned value is one that has already been resolved to its final string
/// form.
pub type PinnedValue = Arc<str>;

/// Represents a constraint added to a resolved environment.
///
/// Its value has already been pinned.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Variantly)]
#[cfg_attr(feature = "parsedbuf-serde", derive(Serialize), serde(untagged))]
pub enum PinnedRequest {
    Pkg(PkgRequest),
    Var(VarRequest<PinnedValue>),
}

impl crate::spec_ops::Named<OptName> for PinnedRequest {
    fn name(&self) -> &OptName {
        match self {
            PinnedRequest::Var(r) => &r.var,
            PinnedRequest::Pkg(r) => r.pkg.name.as_opt_name(),
        }
    }
}

impl std::fmt::Display for PinnedRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pkg(p) => p.fmt(f),
            Self::Var(v) => v.fmt(f),
        }
    }
}

impl From<VarRequest<PinnedValue>> for PinnedRequest {
    fn from(req: VarRequest<PinnedValue>) -> Self {
        Self::Var(req)
    }
}

impl From<PkgRequest> for PinnedRequest {
    fn from(req: PkgRequest) -> Self {
        Self::Pkg(req)
    }
}

impl<'de> Deserialize<'de> for PinnedRequest {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        /// This visitor captures all fields that could be valid
        /// for any request, before deciding at the end which variant
        /// to actually build. We ignore any unrecognized field anyway,
        /// but additionally any field that's recognized must be valid
        /// even if it's not going to be used.
        ///
        /// The purpose of this setup is to enable more meaningful errors
        /// for invalid values that contain original source positions. In
        /// order to achieve this we must parse and validate each field with
        /// the appropriate type as they are visited - which disqualifies the
        /// existing approach to untagged enums which read all fields first
        /// and then goes back and checks them once the variant is determined
        #[derive(Default)]
        struct RequestVisitor {
            // PkgRequest
            pkg: Option<RangeIdent>,
            prerelease_policy: Option<PreReleasePolicy>,
            inclusion_policy: Option<InclusionPolicy>,

            // VarRequest
            var: Option<OptNameBuf>,
            value: Option<String>,
            description: Option<String>,

            // Both
            pin: Option<PinValue>,
            pin_policy: Option<PinPolicy>,
        }

        impl<'de> serde::de::Visitor<'de> for RequestVisitor {
            type Value = PinnedRequest;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a pkg or var request")
            }

            fn visit_map<A>(mut self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                while let Some(mut key) = map.next_key::<Stringified>()? {
                    key.make_ascii_lowercase();
                    match key.as_str() {
                        "pkg" => self.pkg = Some(map.next_value::<RangeIdent>()?),
                        "prereleasepolicy" => {
                            self.prerelease_policy = Some(map.next_value::<PreReleasePolicy>()?)
                        }
                        "ifpresentinbuildenv" => {
                            self.pin_policy = Some(map.next_value::<PinPolicy>()?)
                        }
                        "include" => {
                            self.inclusion_policy = Some(map.next_value::<InclusionPolicy>()?)
                        }
                        "frombuildenv" => self.pin = Some(map.next_value::<PinValue>()?),
                        "var" => {
                            let NameAndValue(name, value) = map.next_value()?;
                            self.var = Some(name);
                            self.value = value;
                        }
                        "value" => self.value = Some(map.next_value::<String>()?),
                        "description" => self.description = Some(map.next_value::<String>()?),
                        _ => {
                            // unrecognized fields are explicitly ignored in case
                            // they were added in a newer version of spk. We assume
                            // that if the api has not been versioned then the desire
                            // is to continue working in this older version
                            map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                match (self.pkg, self.var) {
                    (Some(pkg), None)
                        if self.pin.as_ref().map(PinValue::is_some).unwrap_or_default()
                            && !pkg.version.is_empty() =>
                    {
                        Err(serde::de::Error::custom(format!(
                            "request for `{}` cannot specify a value `/{:#}` when `fromBuildEnv` is specified",
                            pkg.name, pkg.version
                        )))
                    }
                    (Some(pkg), None) => Ok(PinnedRequest::Pkg(PkgRequest {
                        pkg,
                        prerelease_policy: self.prerelease_policy,
                        inclusion_policy: self.inclusion_policy.unwrap_or_default(),
                        pin_policy: self.pin_policy.unwrap_or_default(),
                        pin: self.pin.unwrap_or_default().into_pkg_pin(),
                        required_compat: None,
                        requested_by: Default::default(),
                    })),
                    (None, Some(var)) => {
                        let Some(value) = self.value else {
                            return Err(serde::de::Error::custom(
                                "expected a value for a pinned var request",
                            ));
                        };
                        if value.is_empty() {
                            return Err(serde::de::Error::custom(
                                "a pinned var request must include a non-empty value",
                            ));
                        }
                        Ok(PinnedRequest::Var(VarRequest {
                            var,
                            value: value.into(),
                            description: self.description.clone(),
                        }))
                    }
                    (Some(_), Some(_)) => Err(serde::de::Error::custom(
                        "could not determine request type, it may only contain one of the `pkg` or `var` fields",
                    )),
                    (None, None) => Err(serde::de::Error::custom(
                        "could not determine request type, it must include either a `pkg` or `var` field",
                    )),
                }
            }
        }

        deserializer.deserialize_any(RequestVisitor::default())
    }
}

impl std::fmt::Display for VarRequest<PinnedValue> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // break apart to ensure that new fields are incorporated into this
        // function if they are added in the future
        let Self {
            var,
            value,
            description: _,
        } = self;
        f.write_str("var: ")?;
        var.fmt(f)?;
        f.write_char('/')?;
        value.fmt(f)?;
        Ok(())
    }
}

#[cfg(feature = "parsedbuf-serde")]
impl Serialize for VarRequest<PinnedValue> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;

        let len = if self.description.is_some() { 2 } else { 1 };

        let mut map = serializer.serialize_map(Some(len))?;

        let var = format!("{}/{}", self.var, self.value);
        map.serialize_entry("var", &var)?;

        if self.description.is_some() {
            map.serialize_entry("description", &self.description.clone().unwrap_or_default())?;
        }

        map.end()
    }
}
