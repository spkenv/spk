// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::cmp::{Ord, Ordering};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::str::FromStr;

use itertools::izip;
use serde::{Deserialize, Serialize};

use super::{Version, VERSION_SEP};
use crate::{Error, Result};

#[cfg(test)]
#[path = "./compat_test.rs"]
mod compat_test;

pub const API_STR: &str = "API";
pub const BINARY_STR: &str = "Binary";

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
                "Unknown or unsupported compatibility rule: {}",
                s
            ))),
        }
    }
}

impl TryFrom<&char> for CompatRule {
    type Error = crate::Error;

    fn try_from(c: &char) -> crate::Result<CompatRule> {
        match c {
            'x' => Ok(CompatRule::None),
            'a' => Ok(CompatRule::API),
            'b' => Ok(CompatRule::Binary),
            _ => Err(crate::Error::String(format!(
                "Invalid compatibility rule: {}",
                c
            ))),
        }
    }
}

impl AsRef<str> for CompatRule {
    fn as_ref(&self) -> &str {
        match self {
            CompatRule::None => "x",
            CompatRule::API => "a",
            CompatRule::Binary => "b",
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

#[derive(Debug, Default, Hash, Clone, PartialEq, Eq)]
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

/// Compat specifies the compatilbility contract of a compat number.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Compat(Vec<CompatRuleSet>);

impl Default for Compat {
    fn default() -> Compat {
        // equivalent to "x.a.b"
        Compat(vec![
            CompatRuleSet::single(CompatRule::None),
            CompatRuleSet::single(CompatRule::API),
            CompatRuleSet::single(CompatRule::Binary),
        ])
    }
}

impl std::fmt::Display for Compat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let str_parts: Vec<_> = self.0.iter().map(|r| r.to_string()).collect();
        f.write_str(&str_parts.join(VERSION_SEP))
    }
}

impl TryFrom<&str> for Compat {
    type Error = crate::Error;

    fn try_from(value: &str) -> crate::Result<Self> {
        Self::from_str(value)
    }
}

impl FromStr for Compat {
    type Err = crate::Error;

    fn from_str(value: &str) -> crate::Result<Self> {
        let mut parts = Vec::new();
        for part in value.split('.') {
            let mut rule_set = CompatRuleSet::default();
            for c in part.chars() {
                rule_set.0.insert(CompatRule::try_from(&c)?);
            }
            parts.push(rule_set);
        }
        Ok(Self(parts))
    }
}

impl Compat {
    // True if this is the default compatibility specification
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Create a compat rule set with two parts
    pub fn double(first: CompatRuleSet, second: CompatRuleSet) -> Self {
        Compat(vec![first, second])
    }

    /// Create a compat rule set with three parts
    pub fn triple(first: CompatRuleSet, second: CompatRuleSet, third: CompatRuleSet) -> Self {
        Compat(vec![first, second, third])
    }

    /// Return true if the two version are api compatible by this compat rule.
    pub fn is_api_compatible(&self, base: &Version, other: &Version) -> Compatibility {
        self.check_compat(base, other, CompatRule::API)
    }

    /// Return true if the two version are binary compatible by this compat rule.
    pub fn is_binary_compatible(&self, base: &Version, other: &Version) -> Compatibility {
        self.check_compat(base, other, CompatRule::Binary)
    }

    pub fn render(&self, version: &Version) -> String {
        let parts: Vec<_> = version
            .parts()
            .drain(..self.0.len())
            .map(|p| p.to_string())
            .collect();
        format!("~{}", parts.join(VERSION_SEP))
    }

    fn check_compat(&self, base: &Version, other: &Version, required: CompatRule) -> Compatibility {
        if base == other {
            return Compatibility::Compatible;
        }

        let each = itertools::izip!(self.0.iter(), base.parts(), other.parts());
        for (i, (rule, a, b)) in each.enumerate() {
            if rule.0.contains(&CompatRule::None) {
                if a != b {
                    return Compatibility::Incompatible(format!(
                        "Not compatible with {} [{} at pos {}]",
                        base, self, i
                    ));
                }
                continue;
            }

            if !rule.0.contains(&required) {
                if b == a {
                    continue;
                }
                return Compatibility::Incompatible(format!(
                    "Not {:?} compatible with {} [{} at pos {}]",
                    required, base, self, i
                ));
            }

            if b < a {
                return Compatibility::Incompatible(format!(
                    "Not {:?} compatible with {} [{} at pos {}]",
                    required, base, self, i
                ));
            } else {
                return Compatibility::Compatible;
            }
        }

        Compatibility::Incompatible(format!(
            "Not compatible: {} ({}) [{:?} compatibility not specified]",
            base, self, required,
        ))
    }
}

/// Parse a string as a compatibility specifier.
pub fn parse_compat<S: AsRef<str>>(compat: S) -> crate::Result<Compat> {
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
        let value = String::deserialize(deserializer)?;
        match parse_compat(value) {
            Err(err) => Err(serde::de::Error::custom(err.to_string())),
            Ok(compat) => Ok(compat),
        }
    }
}
