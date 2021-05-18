// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::cmp::{Ord, Ordering};
use std::collections::BTreeSet;
use std::convert::TryFrom;
use std::iter::FromIterator;
use std::str::FromStr;

use itertools::izip;

use super::{Version, VERSION_SEP};

#[cfg(test)]
#[path = "./compat_test.rs"]
mod compat_test;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CompatRule {
    None,
    API,
    ABI,
}

impl std::fmt::Display for CompatRule {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl TryFrom<&char> for CompatRule {
    type Error = crate::Error;

    fn try_from(c: &char) -> crate::Result<CompatRule> {
        match c {
            'x' => Ok(CompatRule::None),
            'a' => Ok(CompatRule::API),
            'b' => Ok(CompatRule::ABI),
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
            CompatRule::ABI => "b",
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
    // enums. For example API is less than ABI because it's considered
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
            Compatibility::Incompatible(msg) => f.write_str(&msg),
        }
    }
}

impl Compatibility {
    pub fn is_ok(&self) -> bool {
        match self {
            Compatibility::Compatible => true,
            _ => false,
        }
    }

    pub fn message<'a>(&'a self) -> &'a str {
        match self {
            Compatibility::Compatible => "",
            Compatibility::Incompatible(msg) => msg.as_ref(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
        CompatRuleSet(BTreeSet::from_iter(vec![first].into_iter()))
    }
    /// Create a compat rule set with two parts
    pub fn double(first: CompatRule, second: CompatRule) -> Self {
        CompatRuleSet(BTreeSet::from_iter(vec![first, second].into_iter()))
    }

    /// Create a compat rule set with three parts
    pub fn triple(first: CompatRule, second: CompatRule, third: CompatRule) -> Self {
        CompatRuleSet(BTreeSet::from_iter(vec![first, second, third].into_iter()))
    }
}

/// Compat specifies the compatilbility contract of a compat number.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Compat(Vec<CompatRuleSet>);

impl std::fmt::Display for Compat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let str_parts: Vec<_> = self.0.iter().map(|r| r.to_string()).collect();
        f.write_str(&str_parts.join(VERSION_SEP))
    }
}

impl FromStr for Compat {
    type Err = crate::Error;

    fn from_str(value: &str) -> crate::Result<Self> {
        let mut parts = Vec::new();
        for part in value.split(".") {
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
        return self.check_compat(base, other, CompatRule::API);
    }

    /// Return true if the two version are binary compatible by this compat rule.
    pub fn is_binary_compatible(&self, base: &Version, other: &Version) -> Compatibility {
        return self.check_compat(base, other, CompatRule::ABI);
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
                    "Not {} compatible with {} [{} at pos {}]",
                    required, base, self, i
                ));
            }

            if b < a {
                return Compatibility::Incompatible(format!(
                    "Not {} compatible with {} [{} at pos {}]",
                    required, base, self, i
                ));
            } else {
                return Compatibility::Compatible;
            }
        }

        Compatibility::Incompatible(format!(
            "Not compatible: {} ({}) [{} compatibility not specified]",
            base, self, required,
        ))
    }
}

/// Parse a string as a compatibility specifier.
pub fn parse_compat<S: AsRef<str>>(compat: S) -> crate::Result<Compat> {
    Compat::from_str(compat.as_ref())
}
