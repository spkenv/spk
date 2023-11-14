// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::cmp::{Ord, Ordering};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::{Error, Result, Version, VERSION_SEP};
use crate::name::{OptNameBuf, PkgNameBuf};
use crate::option_map::OptionMap;
use crate::IsDefault;

#[cfg(test)]
#[path = "./compat_test.rs"]
mod compat_test;

pub const API_COMPAT_STR: &str = "a";
pub const API_STR: &str = "API";
pub const BINARY_COMPAT_STR: &str = "b";
pub const BINARY_STR: &str = "Binary";
pub const NONE_COMPAT_STR: &str = "x";
pub const POST_DELIMITER_STR: &str = "+";
pub const PRE_DELIMITER_STR: &str = "-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompatRule {
    None,
    API,
    Binary,
}

impl std::fmt::Display for CompatRule {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if f.alternate() {
            // Request for alternate (long form) names.
            f.write_str(match self {
                CompatRule::None => unreachable!(),
                CompatRule::API => API_STR,
                CompatRule::Binary => BINARY_STR,
            })
        } else {
            f.write_str(self.as_ref())
        }
    }
}

impl std::str::FromStr for CompatRule {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            API_STR => Ok(Self::API),
            BINARY_STR => Ok(Self::Binary),
            _ => Err(Error::String(format!(
                "Unknown or unsupported compatibility rule: {s}"
            ))),
        }
    }
}

impl AsRef<str> for CompatRule {
    fn as_ref(&self) -> &str {
        match self {
            CompatRule::None => NONE_COMPAT_STR,
            CompatRule::API => API_COMPAT_STR,
            CompatRule::Binary => BINARY_COMPAT_STR,
        }
    }
}

impl PartialOrd for CompatRule {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CompatRule {
    // The current logic requires that there is an order to these
    // enums. For example API is less than Binary because it's considered
    // a subset - aka you cannot provide binary compatibility and not
    // API compatibility
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

/// Denotes whether or not something is compatible.
#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum IncompatibleReason {
    #[strum(
        to_string = "embedded package {embedded} already embedded by another package in solve: {embedded_by}"
    )]
    AlreadyEmbeddedPackage {
        embedded: PkgNameBuf,
        embedded_by: PkgNameBuf,
    },
    #[strum(to_string = "build is deprecated and not requested specifically")]
    BuildDeprecated,
    #[strum(to_string = "building from source is not enabled")]
    BuildFromSourceDisabled,
    #[strum(to_string = "build id does not match")]
    BuildIdMismatch,
    #[strum(to_string = "build id is not a superset")]
    BuildIdNotSuperset,
    #[strum(to_string = "build option {0} has invalid value")]
    BuildOptionMismatch(OptNameBuf),
    #[strum(to_string = "build options {0} not defined in package")]
    BuildOptionsMissing(OptionMap),
    #[strum(to_string = "components {0} not defined in package")]
    ComponentsMissing(BTreeSet<String>),
    #[strum(to_string = "embedded package conflicts with existing package in solve: {pkg}")]
    ConflictingEmbeddedPackage(PkgNameBuf),
    #[strum(
        to_string = "package {0} component {1} embeds {2} and conflicts with existing package in solve: {3}"
    )]
    ConflictingEmbeddedPackageRequirement(PkgNameBuf, String, PkgNameBuf, Box<IncompatibleReason>),
    #[strum(to_string = "package {0} requirement conflicts with existing package in solve: {1}")]
    ConflictingRequirement(PkgNameBuf, Box<IncompatibleReason>),
    #[strum(to_string = "would produce an impossible request")]
    ImpossibleRequest,
    #[strum(to_string = "inclusion policy is not a superset")]
    InclusionPolicyNotSuperset,
    #[strum(to_string = "{0} [INTERNAL ERROR]")]
    InternalError(String),
    #[strum(to_string = "no builds were compatible with a request")]
    NoCompatibleBuilds,
    #[strum(to_string = "recipe options incompatible with state")]
    OptionResolveError,
    #[strum(to_string = "package name does not match")]
    PackageNameMismatch,
    #[strum(to_string = "package is not an embedded package")]
    PackageNotAnEmbeddedPackage,
    #[strum(to_string = "package repo does not match")]
    PackageRepoMismatch,
    #[strum(to_string = "prereleases not allowed")]
    PrereleasesNotAllowed,
    #[strum(to_string = "ranges do not intersect")]
    RangesDoNotIntersect,
    #[strum(to_string = "range is not a superset")]
    RangeNotSuperset,
    #[strum(to_string = "recipe is deprecated in this version")]
    RecipeDeprecated,
    #[strum(to_string = "requirements are not a superset")]
    RequirementsNotSuperset,
    #[strum(to_string = "var option {0} does not equal any of the defined choices")]
    VarOptionIllegalChoice(OptNameBuf),
    #[strum(to_string = "var option {0} doesn't match")]
    VarOptionMismatch(OptNameBuf),
    #[strum(to_string = "var option {0} not defined in package")]
    VarOptionMissing(OptNameBuf),
    #[strum(to_string = "var request is not a superset")]
    VarRequestNotSuperset,
    #[strum(to_string = "var requirement {0} doesn't match")]
    VarRequirementMismatch(OptNameBuf),
    #[strum(to_string = "version range '{version_range}' invalid: {err}")]
    VersionRangeInvalid { version_range: String, err: String },
    #[strum(to_string = "version too high")]
    VersionTooHigh,
    #[strum(to_string = "version too low")]
    VersionTooLow,
    #[strum(to_string = "version doesn't satisfy compatibility requirements")]
    VersionNotCompatible,
    #[strum(to_string = "version not different")]
    VersionNotDifferent,
    #[strum(to_string = "version not equal")]
    VersionNotEqual,
    #[strum(to_string = "version out of range")]
    VersionOutOfRange,
}

/// Denotes whether or not something is compatible.
#[must_use = "this `Compatibility` may be an `Incompatible` variant, which should be handled"]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Compatibility {
    Compatible,
    Incompatible(IncompatibleReason),
}

impl std::fmt::Display for Compatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Compatibility::Compatible => f.write_str(""),
            Compatibility::Incompatible(reason) => reason.fmt(f),
        }
    }
}

impl std::ops::Not for &'_ Compatibility {
    type Output = bool;

    fn not(self) -> Self::Output {
        match self {
            Compatibility::Compatible => false,
            Compatibility::Incompatible(_) => true,
        }
    }
}

impl Compatibility {
    pub fn embedded_conflict(conflicting_package_name: PkgNameBuf) -> Self {
        Compatibility::Incompatible(IncompatibleReason::ConflictingEmbeddedPackage(
            conflicting_package_name,
        ))
    }

    #[inline]
    pub fn is_ok(&self) -> bool {
        matches!(self, &Compatibility::Compatible)
    }

    /// Return true if the compatibility is incompatible.
    pub fn is_err(&self) -> bool {
        match self {
            Compatibility::Compatible => false,
            Compatibility::Incompatible(_) => true,
        }
    }

    /// Unwrap the compatibility, panicking if it is incompatible.
    pub fn unwrap(self) {
        match self {
            Compatibility::Compatible => {}
            Compatibility::Incompatible(reason) => {
                panic!("Unwrapping incompatible compatibility: {reason}")
            }
        }
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompatRuleSet(BTreeSet<CompatRule>);

impl std::fmt::Display for CompatRuleSet {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let strings: Vec<_> = self.0.iter().map(|r| r.to_string()).collect();
        f.write_str(&strings.join(""))
    }
}

impl CompatRuleSet {
    /// Create a compat rule set with one part
    pub fn single(first: CompatRule) -> Self {
        CompatRuleSet(vec![first].into_iter().collect())
    }
    /// Create a compat rule set with two parts
    pub fn double(first: CompatRule, second: CompatRule) -> Self {
        CompatRuleSet(vec![first, second].into_iter().collect())
    }

    /// Create a compat rule set with three parts
    pub fn triple(first: CompatRule, second: CompatRule, third: CompatRule) -> Self {
        CompatRuleSet(vec![first, second, third].into_iter().collect())
    }
}

/// Compat specifies the compatibility contract of a compat number.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Compat {
    parts: Vec<CompatRuleSet>,
    pre: Option<CompatRuleSet>,
    post: Option<CompatRuleSet>,
}

impl Default for Compat {
    fn default() -> Compat {
        // equivalent to "x.a.b"
        Compat {
            parts: vec![
                CompatRuleSet::single(CompatRule::None),
                CompatRuleSet::single(CompatRule::API),
                CompatRuleSet::single(CompatRule::Binary),
            ],
            pre: None,
            post: None,
        }
    }
}

impl std::fmt::Display for Compat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let str_parts: Vec<_> = self.parts.iter().map(|r| r.to_string()).collect();
        f.write_str(&str_parts.join(VERSION_SEP))?;
        if let Some(pre) = &self.pre {
            f.write_str(PRE_DELIMITER_STR)?;
            f.write_str(&pre.to_string())?;
        }
        if let Some(post) = &self.post {
            f.write_str(POST_DELIMITER_STR)?;
            f.write_str(&post.to_string())?;
        }
        Ok(())
    }
}

impl TryFrom<&str> for Compat {
    type Error = super::Error;

    fn try_from(value: &str) -> super::Result<Self> {
        Self::from_str(value)
    }
}

impl FromStr for Compat {
    type Err = super::Error;

    fn from_str(value: &str) -> super::Result<Self> {
        use nom::branch::alt;
        use nom::bytes::complete::tag;
        use nom::combinator::{complete, map};
        use nom::multi::{fold_many0, many1, separated_list1};
        use nom::sequence::preceded;
        use nom::IResult;

        fn compat_none(s: &str) -> IResult<&str, CompatRule> {
            map(tag(NONE_COMPAT_STR), |_| CompatRule::None)(s)
        }

        fn compat_api(s: &str) -> IResult<&str, CompatRule> {
            map(tag(API_COMPAT_STR), |_| CompatRule::API)(s)
        }

        fn compat_binary(s: &str) -> IResult<&str, CompatRule> {
            map(tag(BINARY_COMPAT_STR), |_| CompatRule::Binary)(s)
        }

        fn compat_rule(s: &str) -> IResult<&str, CompatRule> {
            alt((compat_none, compat_api, compat_binary))(s)
        }

        fn compat_rule_set(s: &str) -> IResult<&str, CompatRuleSet> {
            map(many1(compat_rule), |rules| {
                CompatRuleSet(rules.into_iter().collect())
            })(s)
        }

        // Parse the main period-separated list of `CompatRule`s
        let (s, parts) =
            separated_list1(tag(VERSION_SEP), compat_rule_set)(value).map_err(|err| {
                Error::String(format!("Failed to parse compat value '{value}': {err}"))
            })?;

        enum PreOrPost {
            Pre,
            Post,
        }

        fn pre_rule(s: &str) -> IResult<&str, (PreOrPost, CompatRuleSet)> {
            preceded(
                tag(PRE_DELIMITER_STR),
                map(compat_rule_set, |s| (PreOrPost::Pre, s)),
            )(s)
        }

        fn post_rule(s: &str) -> IResult<&str, (PreOrPost, CompatRuleSet)> {
            preceded(
                tag(POST_DELIMITER_STR),
                map(compat_rule_set, |s| (PreOrPost::Post, s)),
            )(s)
        }

        // Parse optional Pre- and Post-`CompatRule`s.
        let (_, compat) = complete(fold_many0(
            alt((pre_rule, post_rule)),
            || Self {
                parts: parts.clone(),
                ..Default::default()
            },
            |mut acc, (pre_or_post, compat_rule)| {
                match pre_or_post {
                    PreOrPost::Pre => acc.pre = Some(compat_rule),
                    PreOrPost::Post => acc.post = Some(compat_rule),
                }
                acc
            },
        ))(s)
        .map_err(|err| {
            Error::String(format!(
                "Failed to parse pre/post compat value '{value}': {err}"
            ))
        })?;

        Ok(compat)
    }
}

impl Compat {
    /// Create a compat rule set with two parts
    pub fn double(first: CompatRuleSet, second: CompatRuleSet) -> Self {
        Compat {
            parts: vec![first, second],
            ..Default::default()
        }
    }

    /// Create a compat rule set with three parts
    pub fn triple(first: CompatRuleSet, second: CompatRuleSet, third: CompatRuleSet) -> Self {
        Compat {
            parts: vec![first, second, third],
            ..Default::default()
        }
    }

    /// Return true if the two versions are api compatible by this compat rule.
    pub fn is_api_compatible(&self, base: &Version, other: &Version) -> Compatibility {
        self.check_compat(base, other, CompatRule::API)
    }

    /// Return true if the two versions are binary compatible by this compat rule.
    pub fn is_binary_compatible(&self, base: &Version, other: &Version) -> Compatibility {
        self.check_compat(base, other, CompatRule::Binary)
    }

    pub fn render(&self, version: &Version) -> String {
        let parts = version
            .parts
            .iter()
            .chain(std::iter::repeat(&0))
            .take(self.parts.len())
            .map(|p| p.to_string());
        format!("~{}", parts.format(VERSION_SEP))
    }

    fn check_compat(&self, base: &Version, other: &Version, required: CompatRule) -> Compatibility {
        // If `base` and `other` only differ by the pre or post parts, then
        // compatibility is determined by our pre/post rules.
        if base.parts == other.parts {
            let pre_matches = base.pre == other.pre;
            let post_matches = base.post == other.post;

            // If no pre/post compat rule is specified, then we default
            // to treating different pre/post releases as compatible.
            if (pre_matches || self.pre.is_none()) && (post_matches || self.post.is_none()) {
                return Compatibility::Compatible;
            }

            for (matches, optruleset) in [(pre_matches, &self.pre), (post_matches, &self.post)] {
                if matches {
                    continue;
                }

                if let Some(ruleset) = optruleset {
                    if ruleset.0.contains(&CompatRule::None) {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible,
                        );
                    }

                    if !ruleset.0.contains(&required) {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible,
                        );
                    }
                }
            }

            // If above logic finds no problems then it is compatible
            return Compatibility::Compatible;
        }

        for (i, rule) in self.parts.iter().enumerate() {
            let a = base.parts.get(i);
            let b = other.parts.get(i);

            if a.is_none() {
                // Handle case where "3.10" is specified and other is "3.10.10".
                // Since 3 == 3 and 10 == 10 and no other version parts are
                // specified, we consider these compatible.
                return Compatibility::Compatible;
            }

            if rule.0.contains(&CompatRule::None) {
                match (a, b) {
                    (Some(a), Some(b)) if a != b => {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible,
                        );
                    }
                    _ => continue,
                }
            }

            if !rule.0.contains(&required) {
                match (a, b) {
                    (Some(a), Some(b)) if a == b => {
                        continue;
                    }
                    (Some(_), Some(_)) => {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible,
                        );
                    }
                    _ => continue,
                }
            }

            match (a, b) {
                (Some(a), Some(b)) if b < a => {
                    return Compatibility::Incompatible(IncompatibleReason::VersionNotCompatible);
                }
                _ => {
                    return Compatibility::Compatible;
                }
            }
        }

        Compatibility::Incompatible(IncompatibleReason::VersionNotCompatible)
    }
}

impl IsDefault for Compat {
    // True if this is the default compatibility specification
    fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// Parse a string as a compatibility specifier.
pub fn parse_compat<S: AsRef<str>>(compat: S) -> super::Result<Compat> {
    Compat::from_str(compat.as_ref())
}

impl Serialize for Compat {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Compat {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CompatVisitor;
        impl<'de> serde::de::Visitor<'de> for CompatVisitor {
            type Value = Compat;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a compatibility specifier (eg: 'x.a.b', 'x.ab')")
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Compat, E>
            where
                E: serde::de::Error,
            {
                Compat::from_str(value).map_err(serde::de::Error::custom)
            }
        }
        deserializer.deserialize_str(CompatVisitor)
    }
}
