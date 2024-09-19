// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod problems;

use std::cmp::{Ord, Ordering};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::str::FromStr;

use indexmap::IndexSet;
use itertools::Itertools;
pub use problems::{
    BuildIdProblem,
    ComponentsMissingProblem,
    ConflictingRequirementProblem,
    ImpossibleRequestProblem,
    InclusionPolicyProblem,
    PackageNameProblem,
    PackageRepoProblem,
    RangeSupersetProblem,
    VarOptionProblem,
    VarRequestProblem,
    VersionForClause,
    VersionNotDifferentProblem,
    VersionNotEqualProblem,
    VersionRangeProblem,
};
use serde::{Deserialize, Serialize};

use super::{Error, Result, TagSet, Version, VERSION_SEP};
use crate::name::{OptNameBuf, PkgNameBuf};
use crate::option_map::OptionMap;
use crate::version_range::WildcardRange;
use crate::{version, IsDefault};

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

/// Compare incompatibility reasons for determining if they can be considered
/// the same problem.
pub trait IsSameReasonAs {
    fn is_same_reason_as(&self, other: &Self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum CompatNotCompatible {
    #[strum(to_string = "Not compatible")]
    NotCompatible,
    #[strum(to_string = "Not {required:?} compatible")]
    NotRequiredCompatibility { required: CompatRule },
}

#[derive(Clone, Debug, Eq, PartialEq, strum::Display)]
pub enum CompatNotCompatibleSpan {
    #[strum(to_string = "[{this_compat} at {desc}: has {has}; requires {requires}]")]
    PreOrPostVersion {
        this_compat: Compat,
        desc: &'static str,
        has: TagSet,
        requires: TagSet,
    },
    #[strum(to_string = "[{this_compat} at pos {pos} ({label}): has {has}; requires {requires}]")]
    VersionPart {
        this_compat: Compat,
        pos: usize,
        label: &'static str,
        has: u32,
        requires: u32,
    },
    #[strum(
        to_string = "[{this_compat} at pos {pos} ({label}): (version) {has} < {requires} (compat)]"
    )]
    VersionPartTooLow {
        this_compat: Compat,
        pos: usize,
        label: &'static str,
        has: u32,
        requires: u32,
    },
    #[strum(to_string = "[{required:?} compatibility not specified]")]
    NotSpecified { required: CompatRule },
}

/// A generic type that implements Display by joining the elements with commas.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommaSeparated<T>(pub T);

impl<T, I> std::fmt::Display for CommaSeparated<T>
where
    for<'a> &'a T: IntoIterator<Item = &'a I>,
    I: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut first = true;
        for item in &self.0 {
            if !first {
                f.write_str(", ")?;
            }
            item.fmt(f)?;
            first = false;
        }
        Ok(())
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
    #[strum(to_string = "{0}")]
    BuildIdMismatch(BuildIdProblem),
    #[strum(to_string = "{0}")]
    BuildIdNotSuperset(BuildIdProblem),
    #[strum(to_string = "invalid value for {name}: {inner_reason}")]
    BuildOptionMismatch {
        name: OptNameBuf,
        inner_reason: Box<IncompatibleReason>,
    },
    #[strum(to_string = "package does not define requested build options: {missing:?}")]
    BuildOptionsMissing(OptionMap),
    #[strum(to_string = "{0}")]
    ComponentsMissing(ComponentsMissingProblem),
    #[strum(to_string = "embedded package conflicts with existing package in solve: {pkg}")]
    ConflictingEmbeddedPackage(PkgNameBuf),
    #[strum(
        to_string = "package {0} component {1} embeds {2} and conflicts with existing package in solve: {3}"
    )]
    ConflictingEmbeddedPackageRequirement(PkgNameBuf, String, PkgNameBuf, Box<IncompatibleReason>),
    #[strum(to_string = "{0}")]
    ConflictingRequirement(ConflictingRequirementProblem),
    #[strum(to_string = "embedded package '{pkg}' is incompatible: {inner_reason}")]
    EmbeddedIncompatible {
        pkg: String,
        inner_reason: Box<IncompatibleReason>,
    },
    #[strum(to_string = "{0}")]
    ImpossibleRequest(ImpossibleRequestProblem),
    #[strum(to_string = "{0}")]
    InclusionPolicyNotSuperset(InclusionPolicyProblem),
    #[strum(to_string = "{0} [INTERNAL ERROR]")]
    InternalError(String),
    #[strum(to_string = "none of {pkg_version}'s builds is compatible with {request}")]
    NoCompatibleBuilds {
        pkg_version: String,
        request: String,
    },
    #[strum(to_string = "doesn't satisfy requested option: {inner_reason}")]
    OptionNotSatisfied {
        inner_reason: Box<IncompatibleReason>,
    },
    #[strum(to_string = "recipe options incompatible with state")]
    OptionResolveError,
    #[strum(to_string = "{0}")]
    PackageNameMismatch(PackageNameProblem),
    #[strum(to_string = "not an embedded package")]
    PackageNotAnEmbeddedPackage,
    #[strum(to_string = "{0}")]
    PackageRepoMismatch(PackageRepoProblem),
    #[strum(to_string = "prereleases not allowed")]
    PrereleasesNotAllowed,
    #[strum(to_string = "{self_valid_range} does not intersect with {other_valid_range}")]
    RangesDoNotIntersect {
        self_valid_range: String,
        other_valid_range: String,
    },
    #[strum(to_string = "{0}")]
    RangeNotSuperset(RangeSupersetProblem),
    #[strum(to_string = "recipe is deprecated in this version")]
    RecipeDeprecated,
    #[strum(to_string = "no request exists for '{name}'")]
    RequirementsNotSuperset { name: OptNameBuf },
    #[strum(to_string = "[{pkg}] {inner_reason}")]
    Restrict {
        pkg: PkgNameBuf,
        inner_reason: Box<IncompatibleReason>,
    },
    #[strum(to_string = "invalid value '{value}'; must be one of {choices:?}")]
    VarOptionIllegalChoice {
        value: String,
        choices: IndexSet<String>,
    },
    #[strum(to_string = "{0}")]
    VarOptionMismatch(VarOptionProblem),
    #[strum(to_string = "var option {0} not defined in package")]
    VarOptionMissing(OptNameBuf),
    #[strum(to_string = "{0}")]
    VarRequestNotSuperset(VarRequestProblem),
    #[strum(to_string = "package wants {var}={requested}; resolve has {name}={value}")]
    VarRequirementMismatch {
        var: OptNameBuf,
        requested: String,
        name: OptNameBuf,
        value: String,
    },
    #[strum(
        to_string = "invalid value '{value}' for option '{option}'; not a valid package request: {err}"
    )]
    VersionRangeInvalid {
        value: String,
        option: PkgNameBuf,
        err: String,
    },
    #[strum(to_string = "{0}")]
    VersionTooHigh(VersionRangeProblem),
    #[strum(to_string = "{0}")]
    VersionTooLow(VersionRangeProblem),
    #[strum(to_string = "{required_compat} with {base} {span}")]
    VersionNotCompatible {
        required_compat: CompatNotCompatible,
        base: Version,
        span: CompatNotCompatibleSpan,
    },
    #[strum(to_string = "{0}")]
    VersionNotDifferent(VersionNotDifferentProblem),
    #[strum(to_string = "{0}")]
    VersionNotEqual(VersionNotEqualProblem),
    #[strum(
        to_string = "out of range: {wildcard} [at pos {pos} ({pos_label}): has {has}; requires {requires}]"
    )]
    VersionOutOfRange {
        wildcard: WildcardRange,
        pos: usize,
        pos_label: &'static str,
        has: u32,
        requires: u32,
    },
}

impl IsSameReasonAs for IncompatibleReason {
    fn is_same_reason_as(&self, other: &Self) -> bool {
        match (self, other) {
            (
                IncompatibleReason::AlreadyEmbeddedPackage {
                    embedded,
                    embedded_by,
                },
                IncompatibleReason::AlreadyEmbeddedPackage {
                    embedded: other_embedded,
                    embedded_by: other_embedded_by,
                },
            ) => embedded == other_embedded && embedded_by == other_embedded_by,
            (IncompatibleReason::BuildDeprecated, IncompatibleReason::BuildDeprecated) => true,
            (
                IncompatibleReason::BuildFromSourceDisabled,
                IncompatibleReason::BuildFromSourceDisabled,
            ) => true,
            (IncompatibleReason::BuildIdMismatch(_), IncompatibleReason::BuildIdMismatch(_)) => {
                true
            }
            (
                IncompatibleReason::BuildIdNotSuperset(_),
                IncompatibleReason::BuildIdNotSuperset(_),
            ) => true,
            (
                IncompatibleReason::BuildOptionMismatch { name: a, .. },
                IncompatibleReason::BuildOptionMismatch { name: b, .. },
            ) => a == b,
            (
                IncompatibleReason::BuildOptionsMissing(a),
                IncompatibleReason::BuildOptionsMissing(b),
            ) => a == b,
            (
                IncompatibleReason::ComponentsMissing(a),
                IncompatibleReason::ComponentsMissing(b),
            ) => a.is_same_reason_as(b),
            (
                IncompatibleReason::ConflictingEmbeddedPackage(a),
                IncompatibleReason::ConflictingEmbeddedPackage(b),
            ) => a == b,
            (
                IncompatibleReason::ConflictingEmbeddedPackageRequirement(a, b, c, d),
                IncompatibleReason::ConflictingEmbeddedPackageRequirement(e, f, g, h),
            ) => a == e && b == f && c == g && d.is_same_reason_as(h),
            (
                IncompatibleReason::ConflictingRequirement(a),
                IncompatibleReason::ConflictingRequirement(b),
            ) => a.is_same_reason_as(b),
            (
                IncompatibleReason::ImpossibleRequest(_),
                IncompatibleReason::ImpossibleRequest(_),
            ) => true,
            (
                IncompatibleReason::InclusionPolicyNotSuperset(_),
                IncompatibleReason::InclusionPolicyNotSuperset(_),
            ) => true,
            (IncompatibleReason::InternalError(a), IncompatibleReason::InternalError(b)) => a == b,
            (
                IncompatibleReason::NoCompatibleBuilds { .. },
                IncompatibleReason::NoCompatibleBuilds { .. },
            ) => true,
            (IncompatibleReason::OptionResolveError, IncompatibleReason::OptionResolveError) => {
                true
            }
            (
                IncompatibleReason::PackageNameMismatch(_),
                IncompatibleReason::PackageNameMismatch(_),
            ) => true,
            (
                IncompatibleReason::PackageNotAnEmbeddedPackage,
                IncompatibleReason::PackageNotAnEmbeddedPackage,
            ) => true,
            (
                IncompatibleReason::PackageRepoMismatch { .. },
                IncompatibleReason::PackageRepoMismatch { .. },
            ) => true,
            (
                IncompatibleReason::PrereleasesNotAllowed,
                IncompatibleReason::PrereleasesNotAllowed,
            ) => true,
            (
                IncompatibleReason::RangesDoNotIntersect { .. },
                IncompatibleReason::RangesDoNotIntersect { .. },
            ) => true,
            (IncompatibleReason::RangeNotSuperset(_), IncompatibleReason::RangeNotSuperset(_)) => {
                true
            }
            (IncompatibleReason::RecipeDeprecated, IncompatibleReason::RecipeDeprecated) => true,
            (
                IncompatibleReason::RequirementsNotSuperset { .. },
                IncompatibleReason::RequirementsNotSuperset { .. },
            ) => true,
            (
                IncompatibleReason::VarOptionIllegalChoice { value: a, .. },
                IncompatibleReason::VarOptionIllegalChoice { value: b, .. },
            ) => a == b,
            (
                IncompatibleReason::VarOptionMismatch(a),
                IncompatibleReason::VarOptionMismatch(b),
            ) => a.is_same_reason_as(b),
            (IncompatibleReason::VarOptionMissing(a), IncompatibleReason::VarOptionMissing(b)) => {
                a == b
            }
            (
                IncompatibleReason::VarRequestNotSuperset(_),
                IncompatibleReason::VarRequestNotSuperset(_),
            ) => true,
            (
                IncompatibleReason::VarRequirementMismatch { var: a, .. },
                IncompatibleReason::VarRequirementMismatch { var: b, .. },
            ) => a == b,
            (
                IncompatibleReason::VersionRangeInvalid {
                    option: a, err: b, ..
                },
                IncompatibleReason::VersionRangeInvalid {
                    option: c, err: d, ..
                },
            ) => a == c && b == d,
            (IncompatibleReason::VersionTooHigh(_), IncompatibleReason::VersionTooHigh(_)) => true,
            (IncompatibleReason::VersionTooLow(_), IncompatibleReason::VersionTooLow(_)) => true,
            (
                IncompatibleReason::VersionNotCompatible { .. },
                IncompatibleReason::VersionNotCompatible { .. },
            ) => true,
            (
                IncompatibleReason::VersionNotDifferent(_),
                IncompatibleReason::VersionNotDifferent(_),
            ) => true,
            (IncompatibleReason::VersionNotEqual(_), IncompatibleReason::VersionNotEqual(_)) => {
                true
            }
            (
                IncompatibleReason::VersionOutOfRange { .. },
                IncompatibleReason::VersionOutOfRange { .. },
            ) => true,
            _ => false,
        }
    }
}

/// Denotes whether or not something is compatible.
#[must_use = "this `Compatibility` may be an `Incompatible` variant, which should be handled"]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Compatibility {
    Compatible,
    Incompatible(IncompatibleReason),
}

impl IsSameReasonAs for Compatibility {
    fn is_same_reason_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Compatibility::Incompatible(a), Compatibility::Incompatible(b)) => {
                a.is_same_reason_as(b)
            }
            _ => false,
        }
    }
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

            for (matches, optruleset, desc, a, b) in [
                (pre_matches, &self.pre, "pre", &base.pre, &other.pre),
                (post_matches, &self.post, "post", &base.post, &other.post),
            ] {
                if matches {
                    continue;
                }

                if let Some(ruleset) = optruleset {
                    if ruleset.0.contains(&CompatRule::None) {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible {
                                required_compat: CompatNotCompatible::NotCompatible,
                                base: base.clone(),
                                span: CompatNotCompatibleSpan::PreOrPostVersion {
                                    this_compat: self.clone(),
                                    desc,
                                    has: b.clone(),
                                    requires: a.clone(),
                                },
                            },
                        );
                    }

                    if !ruleset.0.contains(&required) {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible {
                                required_compat: CompatNotCompatible::NotRequiredCompatibility {
                                    required,
                                },
                                base: base.clone(),
                                span: CompatNotCompatibleSpan::PreOrPostVersion {
                                    this_compat: self.clone(),
                                    desc,
                                    has: b.clone(),
                                    requires: a.clone(),
                                },
                            },
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
                            IncompatibleReason::VersionNotCompatible {
                                required_compat: CompatNotCompatible::NotCompatible,
                                base: base.clone(),
                                span: CompatNotCompatibleSpan::VersionPart {
                                    this_compat: self.clone(),
                                    pos: i + 1,
                                    label: version::get_version_position_label(i),
                                    has: *b,
                                    requires: *a,
                                },
                            },
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
                    (Some(a), Some(b)) => {
                        return Compatibility::Incompatible(
                            IncompatibleReason::VersionNotCompatible {
                                required_compat: CompatNotCompatible::NotCompatible,
                                base: base.clone(),
                                span: CompatNotCompatibleSpan::VersionPart {
                                    this_compat: self.clone(),
                                    pos: i + 1,
                                    label: version::get_version_position_label(i),
                                    has: *b,
                                    requires: *a,
                                },
                            },
                        );
                    }
                    _ => continue,
                }
            }

            match (a, b) {
                (Some(a), Some(b)) if b < a => {
                    return Compatibility::Incompatible(IncompatibleReason::VersionNotCompatible {
                        required_compat: CompatNotCompatible::NotRequiredCompatibility { required },
                        base: base.clone(),
                        span: CompatNotCompatibleSpan::VersionPartTooLow {
                            this_compat: self.clone(),
                            pos: i + 1,
                            label: version::get_version_position_label(i),
                            has: *b,
                            requires: *a,
                        },
                    });
                }
                _ => {
                    return Compatibility::Compatible;
                }
            }
        }

        Compatibility::Incompatible(IncompatibleReason::VersionNotCompatible {
            required_compat: CompatNotCompatible::NotCompatible,
            base: base.clone(),
            span: CompatNotCompatibleSpan::NotSpecified { required },
        })
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
