// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{
    collections::HashSet,
    convert::TryInto,
    fmt::{Display, Write},
    hash::Hash,
    ops::Sub,
    str::FromStr,
};

use enum_dispatch::enum_dispatch;
use itertools::Itertools;

use crate::{Error, Result};

use super::{parse_version, CompatRule, Compatibility, Spec, Version, VERSION_SEP};

#[cfg(test)]
#[path = "./version_range_test.rs"]
mod version_range_test;

pub const VERSION_RANGE_SEP: &str = ",";

/// The generic trait for all range implementations.
///
/// This is not public API as the VersionRange enum is used
/// as the public interface, which can be used to identify
/// which range type is actually being used
#[enum_dispatch]
pub trait Ranged: Display + Clone + Into<VersionRange> {
    /// The lower, inclusive bound for this range
    fn greater_or_equal_to(&self) -> Option<Version>;
    /// The upper bound for this range
    fn less_than(&self) -> Option<Version>;
    /// Return true if the given package spec satisfies this version range with the given compatibility.
    fn is_satisfied_by(&self, spec: &Spec, required: CompatRule) -> Compatibility;

    /// If applicable, return the broken down set of rules for this range
    fn rules(&self) -> HashSet<VersionRange> {
        let mut out = HashSet::with_capacity(1);
        out.insert(self.clone().into());
        out
    }

    /// Return true if the given version seems applicable to this range
    ///
    /// Versions that are applicable are not necessarily satisfactory, but
    /// this cannot be fully determined without a complete package spec.
    fn is_applicable(&self, other: &Version) -> Compatibility {
        if let Some(gt) = self.greater_or_equal_to() {
            if other < &gt {
                return Compatibility::Incompatible("version too low".to_string());
            }
        }
        if let Some(lt) = self.less_than() {
            if other >= &lt {
                return Compatibility::Incompatible("version too high".to_string());
            }
        }
        Compatibility::Compatible
    }

    fn contains(&self, other: impl Ranged) -> Compatibility {
        let self_lower = self.greater_or_equal_to();
        let self_upper = self.less_than();
        let other_lower = other.greater_or_equal_to();
        let other_upper = other.less_than();

        if let (Some(self_lower), Some(other_lower)) = (self_lower, other_lower) {
            if self_lower > other_lower {
                return Compatibility::Incompatible(format!(
                    "{} represents a wider range than allowed by {}",
                    other, self
                ));
            }
        }
        if let (Some(self_upper), Some(other_upper)) = (self_upper, other_upper) {
            if self_upper < other_upper {
                return Compatibility::Incompatible(format!(
                    "{} represents a wider range than allowed by {}",
                    other, self
                ));
            }
        }

        self.intersects(other)
    }

    fn intersects(&self, other: impl Ranged) -> Compatibility {
        let self_lower = self.greater_or_equal_to();
        let self_upper = self.less_than();
        let other_lower = other.greater_or_equal_to();
        let other_upper = other.less_than();

        if let (Some(self_upper), Some(other_lower)) = (self_upper, other_lower) {
            if self_upper < other_lower {
                return Compatibility::Incompatible(format!(
                    "{} does not intersect with {}, all versions too high",
                    other, self
                ));
            }
        }
        if let (Some(self_lower), Some(other_upper)) = (self_lower, other_upper) {
            if self_lower > other_upper {
                return Compatibility::Incompatible(format!(
                    "{} does not intersect with {}, all versions too low",
                    other, self
                ));
            }
        }

        Compatibility::Compatible
    }
}

impl<T: Ranged> Ranged for &T {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Ranged::greater_or_equal_to(*self)
    }
    fn less_than(&self) -> Option<Version> {
        Ranged::less_than(*self)
    }
    fn is_satisfied_by(&self, spec: &Spec, required: CompatRule) -> Compatibility {
        Ranged::is_satisfied_by(*self, spec, required)
    }
}

/// Specifies a range of version numbers by inclusion or exclusion
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[enum_dispatch(Ranged)]
pub enum VersionRange {
    Compat(CompatRange),
    Exact(ExactVersion),
    Excluded(ExcludedVersion),
    Filter(VersionFilter),
    GreaterThan(GreaterThanRange),
    GreaterThanOrEqualTo(GreaterThanOrEqualToRange),
    LessThan(LessThanRange),
    LessThanOrEqualTo(LessThanOrEqualToRange),
    LowestSpecified(LowestSpecifiedRange),
    Semver(SemverRange),
    Wildcard(WildcardRange),
}

impl Display for VersionRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            VersionRange::Compat(vr) => vr.fmt(f),
            VersionRange::Exact(vr) => vr.fmt(f),
            VersionRange::Excluded(vr) => vr.fmt(f),
            VersionRange::Filter(vr) => vr.fmt(f),
            VersionRange::GreaterThan(vr) => vr.fmt(f),
            VersionRange::GreaterThanOrEqualTo(vr) => vr.fmt(f),
            VersionRange::LessThan(vr) => vr.fmt(f),
            VersionRange::LessThanOrEqualTo(vr) => vr.fmt(f),
            VersionRange::LowestSpecified(vr) => vr.fmt(f),
            VersionRange::Semver(vr) => vr.fmt(f),
            VersionRange::Wildcard(vr) => vr.fmt(f),
        }
    }
}

impl<T: Ranged> From<&T> for VersionRange {
    fn from(other: &T) -> Self {
        other.to_owned().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SemverRange {
    minimum: Version,
}

impl SemverRange {
    pub fn new_version_range<V: TryInto<Version, Error = Error>>(
        minimum: V,
    ) -> Result<VersionRange> {
        Ok(VersionRange::Semver(SemverRange {
            minimum: minimum.try_into()?,
        }))
    }
}

impl Ranged for SemverRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.minimum.clone())
    }

    fn less_than(&self) -> Option<Version> {
        let mut parts = self.minimum.parts.clone();
        for (i, p) in parts.iter().enumerate() {
            if p == &0 {
                continue;
            }
            parts[i] = p + 1;
            return Some(Version::from_parts(parts.drain(..i + 1)));
        }

        if let Some(last) = parts.last_mut() {
            *last += 1;
        }
        Some(Version::from_parts(parts))
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }
}

impl Display for SemverRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_char('^')?;
        f.write_str(&self.minimum.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WildcardRange {
    specified: usize,
    parts: Vec<Option<u32>>,
}

impl WildcardRange {
    pub fn new_version_range<S: AsRef<str>>(minimum: S) -> Result<VersionRange> {
        let mut parts = Vec::new();
        for part in minimum.as_ref().split(VERSION_SEP) {
            if part == "*" {
                parts.push(None);
                continue;
            }
            match part.parse() {
                Ok(num) => parts.push(Some(num)),
                Err(_) => {
                    return Err(Error::String(format!(
                        "must consist only of numbers and '*', got: {}",
                        minimum.as_ref(),
                    )));
                }
            }
        }
        let range = WildcardRange {
            specified: parts.len(),
            parts,
        };
        if range.parts.iter().filter(|p| p.is_none()).count() != 1 {
            return Err(Error::String(format!(
                "Expected exactly one wildcard in version range, got: {}",
                range
            )));
        }
        Ok(VersionRange::Wildcard(range))
    }
}

impl Ranged for WildcardRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        let parts = self
            .parts
            .clone()
            .into_iter()
            .map(|p| p.unwrap_or(0))
            .collect_vec();
        Some(Version::from_parts(parts.into_iter()))
    }

    fn less_than(&self) -> Option<Version> {
        let mut parts = self.parts.clone().into_iter().flatten().collect_vec();
        if let Some(last) = parts.last_mut() {
            *last += 1;
        } else {
            return None;
        }
        Some(Version::from_parts(parts.into_iter()))
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        for (i, (a, b)) in self.parts.iter().zip(&version.parts).enumerate() {
            if let Some(a) = a {
                if a != b {
                    return Compatibility::Incompatible(format!(
                        "Out of range: {} [at pos {}]",
                        self, i
                    ));
                }
            }
        }

        Compatibility::Compatible
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }
}

impl Display for WildcardRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let s = self
            .parts
            .iter()
            .map(|p| match p {
                Some(p) => p.to_string(),
                None => "*".to_string(),
            })
            .collect_vec()
            .join(VERSION_SEP);
        f.write_str(&s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LowestSpecifiedRange {
    specified: usize,
    base: Version,
}

impl LowestSpecifiedRange {
    pub fn new_version_range<S: AsRef<str>>(minimum: S) -> Result<VersionRange> {
        let range = Self {
            specified: minimum.as_ref().split(VERSION_SEP).count(),
            base: parse_version(minimum.as_ref())?,
        };
        if range.specified < 2 {
            Err(Error::String(format!(
                "Expected at least two digits in version range, got: {}",
                minimum.as_ref()
            )))
        } else {
            Ok(VersionRange::LowestSpecified(range))
        }
    }
}

impl Ranged for LowestSpecifiedRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.base.clone())
    }

    fn less_than(&self) -> Option<Version> {
        let mut parts = self.base.parts[..self.specified - 1].to_vec();
        if let Some(last) = parts.last_mut() {
            *last += 1;
        }
        Some(Version::from_parts(parts.clone()))
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }
}

impl Display for LowestSpecifiedRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let base_str = self.base.parts[..self.specified]
            .iter()
            .map(ToString::to_string)
            .collect_vec()
            .join(VERSION_SEP);
        f.write_char('~')?;
        f.write_str(&base_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GreaterThanRange {
    bound: Version,
}

impl GreaterThanRange {
    pub fn new_version_range<V: TryInto<Version, Error = Error>>(
        boundary: V,
    ) -> Result<VersionRange> {
        Ok(VersionRange::GreaterThan(Self {
            bound: boundary.try_into()?,
        }))
    }
}

impl Ranged for GreaterThanRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.bound.clone())
    }

    fn less_than(&self) -> Option<Version> {
        None
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        if version <= &self.bound {
            return Compatibility::Incompatible(format!("Not {} [too low]", self));
        }
        Compatibility::Compatible
    }
}

impl Display for GreaterThanRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_char('>')?;
        f.write_str(&self.bound.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LessThanRange {
    bound: Version,
}

impl LessThanRange {
    pub fn new_version_range<V: TryInto<Version, Error = Error>>(
        boundary: V,
    ) -> Result<VersionRange> {
        Ok(VersionRange::LessThan(Self {
            bound: boundary.try_into()?,
        }))
    }
}

impl Ranged for LessThanRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        None
    }

    fn less_than(&self) -> Option<Version> {
        Some(self.bound.clone())
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        if version >= &self.bound {
            return Compatibility::Incompatible(format!("Not {} [too high]", self));
        }
        Compatibility::Compatible
    }
}

impl Display for LessThanRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_char('<')?;
        f.write_str(&self.bound.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GreaterThanOrEqualToRange {
    bound: Version,
}

impl GreaterThanOrEqualToRange {
    pub fn new_version_range<V: TryInto<Version, Error = Error>>(
        boundary: V,
    ) -> Result<VersionRange> {
        Ok(VersionRange::GreaterThanOrEqualTo(Self {
            bound: boundary.try_into()?,
        }))
    }
}

impl Ranged for GreaterThanOrEqualToRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.bound.clone())
    }

    fn less_than(&self) -> Option<Version> {
        None
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        if version < &self.bound {
            return Compatibility::Incompatible(format!("Not {} [too low]", self));
        }
        Compatibility::Compatible
    }
}

impl Display for GreaterThanOrEqualToRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(">=")?;
        f.write_str(&self.bound.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LessThanOrEqualToRange {
    bound: Version,
}

impl LessThanOrEqualToRange {
    pub fn new_version_range<V: TryInto<Version, Error = Error>>(
        boundary: V,
    ) -> Result<VersionRange> {
        Ok(VersionRange::LessThanOrEqualTo(Self {
            bound: boundary.try_into()?,
        }))
    }
}

impl Ranged for LessThanOrEqualToRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        None
    }

    fn less_than(&self) -> Option<Version> {
        None
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        if version > &self.bound {
            return Compatibility::Incompatible(format!("Not {} [too high]", self));
        }
        Compatibility::Compatible
    }
}

impl Display for LessThanOrEqualToRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("<=")?;
        f.write_str(&self.bound.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExactVersion {
    version: Version,
}

impl ExactVersion {
    pub fn version_range(version: Version) -> VersionRange {
        VersionRange::Exact(Self { version })
    }
}

impl From<Version> for ExactVersion {
    fn from(version: Version) -> Self {
        Self { version }
    }
}

impl Ranged for ExactVersion {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.version.clone())
    }

    fn less_than(&self) -> Option<Version> {
        let mut parts = self.version.parts.clone();
        if let Some(last) = parts.last_mut() {
            *last += 1;
        }
        Some(Version::from_parts(parts))
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        if self.version.parts != spec.pkg.version.parts {
            return Compatibility::Incompatible(format!(
                "{} !! {} [not equal]",
                &spec.pkg.version, self
            ));
        }

        if self.version.pre != spec.pkg.version.pre {
            return Compatibility::Incompatible(format!(
                "{} !! {} [not equal @ prerelease]",
                &spec.pkg.version, self
            ));
        }
        // each post release tag must be exact if specified
        for (name, value) in self.version.post.iter() {
            if let Some(v) = spec.pkg.version.post.get(name) {
                if v == value {
                    continue;
                }
            }
            return Compatibility::Incompatible(format!(
                "{} !! {} [not equal @ postrelease]",
                &spec.pkg.version, self
            ));
        }
        Compatibility::Compatible
    }
}

impl Display for ExactVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_char('=')?;
        f.write_str(&self.version.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExcludedVersion {
    specified: usize,
    base: Version,
}

impl ExcludedVersion {
    pub fn new_version_range<S: AsRef<str>>(exclude: S) -> Result<VersionRange> {
        let range = Self {
            specified: exclude.as_ref().split(VERSION_SEP).count(),
            base: parse_version(exclude)?,
        };
        Ok(VersionRange::Excluded(range))
    }
}

impl Ranged for ExcludedVersion {
    fn greater_or_equal_to(&self) -> Option<Version> {
        None
    }

    fn less_than(&self) -> Option<Version> {
        None
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        // Is some part of the specified version different?
        if version
            .parts
            .iter()
            .zip(self.base.parts.iter())
            .take(self.specified)
            .any(|(l, r)| l != r)
        {
            return Compatibility::Compatible;
        }

        // To mirror `ExactVersion`, different post releases are unequal,
        // but unspecified post release is considered equal.
        if !self.base.post.is_empty() && self.base.post != version.post {
            return Compatibility::Compatible;
        }

        Compatibility::Incompatible(format!("excluded [{}]", self))
    }

    fn is_satisfied_by(&self, spec: &Spec, _required: CompatRule) -> Compatibility {
        self.is_applicable(&spec.pkg.version)
    }
}

impl Display for ExcludedVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let base_str = self
            .base
            .parts
            .iter()
            .take(self.specified)
            .map(ToString::to_string)
            .collect_vec()
            .join(VERSION_SEP);
        f.write_str("!=")?;
        f.write_str(&base_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CompatRange {
    base: Version,
    /// if unset, the required compatibility is based on the type
    /// of package being validated. Source packages require api
    /// compat and binary packages require binary compat.
    required: Option<CompatRule>,
}

impl CompatRange {
    pub fn new_version_range<R: AsRef<str>>(range: R) -> Result<VersionRange> {
        let range = range.as_ref();
        let compat_range = match range.rsplit_once(':') {
            Some((prefix, version)) => Self {
                base: version.try_into()?,
                required: Some(CompatRule::from_str(prefix)?),
            },
            None => Self {
                base: range.try_into()?,
                required: None,
            },
        };
        Ok(VersionRange::Compat(compat_range))
    }
}

impl Ranged for CompatRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.base.clone())
    }

    fn less_than(&self) -> Option<Version> {
        None
    }

    fn is_satisfied_by(&self, spec: &Spec, mut required: CompatRule) -> Compatibility {
        if let Some(r) = self.required {
            required = r;
        }
        match required {
            CompatRule::None => Compatibility::Compatible,
            CompatRule::API => spec.compat.is_api_compatible(&self.base, &spec.pkg.version),
            CompatRule::Binary => spec
                .compat
                .is_binary_compatible(&self.base, &spec.pkg.version),
        }
    }
}

impl Display for CompatRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(r) = self.required {
            // get the alternate, long form representation
            // as this is what we expect when parsing
            // (eg 'Binary' instead of 'b')
            f.write_fmt(format_args!("{:#}", r))?;
            f.write_char(':')?;
        }
        f.write_str(&self.base.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct VersionFilter {
    rules: HashSet<VersionRange>,
}

#[allow(clippy::derive_hash_xor_eq)]
impl Hash for VersionFilter {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let rules = self.sorted_rules();
        rules.hash(state)
    }
}

impl VersionFilter {
    pub fn single(item: VersionRange) -> Self {
        let mut filter = Self::default();
        filter.rules.insert(item);
        filter
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        !self.rules.iter().any(|r| match r {
            VersionRange::Filter(f) => !f.is_empty(),
            _ => true,
        })
    }

    pub(crate) fn sorted_rules(&self) -> Vec<String> {
        let mut rules = self.rules.iter().map(|r| r.to_string()).collect_vec();
        rules.sort_unstable();
        rules
    }

    /// Reduce this range by the scope of another
    ///
    /// This version range will become restricted to the intersection
    /// of the current version range and the other.
    pub fn restrict(&mut self, other: impl Ranged) -> Result<()> {
        // FIXME: de-duplicating by strings is less than ideal
        let rendered = other.to_string();
        for rule in self.rules.iter() {
            if rendered == rule.to_string() {
                return Ok(());
            }
        }

        let compat = self.intersects(&other);
        if let Compatibility::Incompatible(msg) = compat {
            return Err(Error::String(msg));
        }

        self.rules.insert(other.into());
        Ok(())
    }
}

impl Ranged for VersionFilter {
    fn greater_or_equal_to(&self) -> Option<Version> {
        self.rules
            .iter()
            .filter_map(|r| r.greater_or_equal_to())
            .max()
    }

    fn less_than(&self) -> Option<Version> {
        self.rules.iter().filter_map(|r| r.less_than()).min()
    }

    /// Return true if the given version number is applicable to this range.
    ///
    /// Versions that are applicable are not necessarily satisfactory, but
    /// this cannot be fully determined without a complete package spec.
    fn is_applicable(&self, other: &Version) -> Compatibility {
        for r in self.rules.iter() {
            let compat = r.is_applicable(other);
            if !compat.is_ok() {
                return compat;
            }
        }
        Compatibility::Compatible
    }

    /// Return true if the given package spec satisfies this version range.
    fn is_satisfied_by(&self, spec: &Spec, required: CompatRule) -> Compatibility {
        for rule in self.rules.iter() {
            let compat = rule.is_satisfied_by(spec, required);
            if !compat.is_ok() {
                return compat;
            }
        }

        Compatibility::Compatible
    }

    fn contains(&self, other: impl Ranged) -> Compatibility {
        let new_rules = other.rules().sub(&self.rules);
        for new_rule in new_rules.iter() {
            for old_rule in self.rules.iter() {
                let compat = old_rule.contains(&new_rule);
                if !compat.is_ok() {
                    return compat;
                }
            }
        }

        Compatibility::Compatible
    }

    fn intersects(&self, other: impl Ranged) -> Compatibility {
        let new_rules = other.rules().sub(&self.rules);
        for new_rule in new_rules {
            for old_rule in self.rules.iter() {
                let compat = old_rule.intersects(&new_rule);
                if !compat.is_ok() {
                    return compat;
                }
            }
        }

        Compatibility::Compatible
    }

    fn rules(&self) -> HashSet<VersionRange> {
        self.rules.clone()
    }
}

impl Display for VersionFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut rules = self
            .rules
            .iter()
            .filter(|r| match r {
                VersionRange::Filter(f) => !f.is_empty(),
                _ => true,
            })
            .map(|r| {
                if f.alternate() {
                    format!("{:#}", r)
                } else {
                    r.to_string()
                }
            })
            .collect_vec();
        rules.sort_unstable();

        let s = rules.join(VERSION_RANGE_SEP);
        f.write_str(&s)
    }
}

impl FromStr for VersionFilter {
    type Err = Error;

    fn from_str(range: &str) -> Result<Self> {
        let mut out = VersionFilter::default();
        if range.is_empty() {
            return Ok(out);
        }
        for rule_str in range.split(VERSION_RANGE_SEP) {
            let rule = if rule_str.is_empty() {
                return Err(Error::String(format!(
                    "Empty segment not allowed in version range, got: {}",
                    range
                )));
            } else if let Some(end) = rule_str.strip_prefix('^') {
                SemverRange::new_version_range(end)?
            } else if let Some(end) = rule_str.strip_prefix('~') {
                LowestSpecifiedRange::new_version_range(end)?
            } else if let Some(end) = rule_str.strip_prefix(">=") {
                GreaterThanOrEqualToRange::new_version_range(end)?
            } else if let Some(end) = rule_str.strip_prefix("<=") {
                LessThanOrEqualToRange::new_version_range(end)?
            } else if let Some(end) = rule_str.strip_prefix('>') {
                GreaterThanRange::new_version_range(end)?
            } else if let Some(end) = rule_str.strip_prefix('<') {
                LessThanRange::new_version_range(end)?
            } else if let Some(end) = rule_str.strip_prefix('=') {
                let version = Version::from_str(end)?;
                ExactVersion::version_range(version)
            } else if let Some(end) = rule_str.strip_prefix("!=") {
                ExcludedVersion::new_version_range(end)?
            } else if rule_str.contains('*') {
                WildcardRange::new_version_range(rule_str)?
            } else {
                CompatRange::new_version_range(rule_str)?
            };
            out.rules.insert(rule);
        }

        Ok(out)
    }
}

pub fn parse_version_range<S: AsRef<str>>(range: S) -> Result<VersionRange> {
    let filter = VersionFilter::from_str(range.as_ref())?;
    Ok(VersionRange::Filter(filter))
}
