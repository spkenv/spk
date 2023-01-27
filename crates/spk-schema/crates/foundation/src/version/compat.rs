// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::cmp::{Ord, Ordering};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::str::FromStr;

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use super::{Error, Result, Version, VERSION_SEP};
use crate::version;

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
#[derive(Clone, Debug)]
pub enum Compatibility {
    Compatible,
    Incompatible(String),
}

impl std::fmt::Display for Compatibility {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Compatibility::Compatible => f.write_str(""),
            Compatibility::Incompatible(msg) => f.write_str(msg),
        }
    }
}

impl std::ops::Not for &'_ Compatibility {
    type Output = bool;

    fn not(self) -> Self::Output {
        match self {
            super::Compatibility::Compatible => false,
            super::Compatibility::Incompatible(_) => true,
        }
    }
}

impl Compatibility {
    pub fn is_ok(&self) -> bool {
        matches!(self, &Compatibility::Compatible)
    }

    pub fn message(&self) -> &str {
        match self {
            Compatibility::Compatible => "",
            Compatibility::Incompatible(msg) => msg.as_ref(),
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
    // True if this is the default compatibility specification
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

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
                        return Compatibility::Incompatible(format!(
                            "Not compatible with {base} [{self} at {desc}: has {}, requires {}]",
                            b.to_string(),
                            a.to_string()
                        ));
                    }

                    if !ruleset.0.contains(&required) {
                        return Compatibility::Incompatible(format!(
                            "Not {:?} compatible with {base} [{self} at {desc}: has {}, requires {}]",
                            required,
                            b.to_string(),
                            a.to_string()
                        ));
                    }
                }
            }

            // If above logic finds no problems then it is compatible
            return Compatibility::Compatible;
        }

        for (i, rule) in self.parts.iter().enumerate() {
            let a = base.parts.get(i);
            let b = other.parts.get(i);
            if rule.0.contains(&CompatRule::None) {
                match (a, b) {
                    (Some(a), Some(b)) if a != b => {
                        return Compatibility::Incompatible(format!(
                            "Not compatible with {base} [{self} at pos {} ({}): has {b}, requires {a}]",
                            i + 1,
                            version::get_version_position_label(i),
                        ));
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
                        return Compatibility::Incompatible(format!(
                            "Not {:?} compatible with {base} [{self} at pos {} ({}): has {b}, requires {a}]",
                            required,
                            i + 1,
                            version::get_version_position_label(i),
                        ));
                    }
                    _ => continue,
                }
            }

            match (a, b) {
                (Some(a), Some(b)) if b < a => {
                    return Compatibility::Incompatible(format!(
                        "Not {:?} compatible with {base} [{self} at pos {} ({}): (version) {b} < {a} (compat)]",
                        required,
                        i + 1,
                        version::get_version_position_label(i),
                    ));
                }
                _ => {
                    return Compatibility::Compatible;
                }
            }
        }

        Compatibility::Incompatible(format!(
            "Not compatible: {base} ({self}) [{required:?} compatibility not specified]",
        ))
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
