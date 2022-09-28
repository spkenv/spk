// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;
use std::convert::{TryFrom, TryInto};
use std::fmt::{Display, Write};
use std::hash::Hash;
use std::ops::Sub;
use std::str::FromStr;

use enum_dispatch::enum_dispatch;
use itertools::Itertools;

use self::intersection::{CombineWith, ValidRange};
use crate::spec_ops::RecipeOps;
use crate::version::{get_version_position_label, CompatRule, Compatibility, Version, VERSION_SEP};

mod error;
mod intersection;
pub mod parsing;

pub use error::{Error, Result};

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
    fn is_satisfied_by<Spec>(&self, spec: &Spec, _required: CompatRule) -> Compatibility
    where
        Spec: RecipeOps,
    {
        self.is_applicable(spec.version())
    }

    /// If applicable, return the broken down set of rules for this range
    fn rules(&self) -> BTreeSet<VersionRange> {
        let mut out = BTreeSet::new();
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
                return Compatibility::Incompatible(format!("version too low for >= {gt}"));
            }
        }
        if let Some(lt) = self.less_than() {
            if other >= &lt {
                return Compatibility::Incompatible(format!("version too high for < {lt}"));
            }
        }
        Compatibility::Compatible
    }

    /// Test that the set of all valid versions in self is a superset of
    /// all valid versions in other.
    fn contains<R: Ranged>(&self, other: R) -> Compatibility {
        match (self.get_compat_rule(), other.get_compat_rule()) {
            (Some(_), None) => {
                // Allow `Binary:1.2.3` to contain `=1.2.3` so that these two
                // ranges can be simplified to just `=1.2.3`.
                let other_v = match other.clone().into() {
                    VersionRange::DoubleEquals(v) => Some(v.version),
                    VersionRange::Equals(v) => Some(v.version),
                    _ => None,
                };

                let contains = if let Some(other_v) = other_v {
                    if let VersionRange::Compat(c) = self.clone().into() {
                        c.base == other_v
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !contains {
                    return Compatibility::Incompatible(format!(
                        "{self} has stronger compatibility requirements than {other}"
                    ));
                }
            }
            (Some(x), Some(y)) if x > y => {
                return Compatibility::Incompatible(format!(
                    "{self} has stronger compatibility requirements than {other}"
                ));
            }
            _ => {}
        };

        let self_lower = self.greater_or_equal_to();
        let self_upper = self.less_than();
        let other_lower = other.greater_or_equal_to();
        let other_upper = other.less_than();

        for (index, (left_bound, right_bound, right_opposite_bound)) in [
            // Order important here! self > other for lower bound
            (&self_lower, &other_lower, &other_upper),
            // Order important here! other > self for upper bound
            (&other_upper, &self_upper, &self_lower),
        ]
        .iter()
        .enumerate()
        {
            // Check that the left bound `Version` contains the right bound `Version`.
            //
            // Using rust range syntax:
            // iter 0 (lower bounds) --  "self.._".contains("other..other_opposite");
            // iter 1 (upper bounds) -- "_..other".contains("self_opposite..self"  );
            //                          |--------|          |---------------------|
            //                            "left"                   "right"
            match (&left_bound, &right_bound, &right_opposite_bound) {
                (None, None, _) => {
                    // neither is bounded
                }
                (None, Some(_), None) => {
                    // <3.0 does not contain >2.0
                    return Compatibility::Incompatible(format!(
                        "[case 1,{index}] {self} does not contain {other}"
                    ));
                }
                (None, Some(right_bound), Some(right_opposite_bound)) => {
                    // For "<2.0" to contain "=1.0", 2.0 must be >= 1.0.
                    //    ..2.0    vs    1.0..1.0+ε
                    //
                    // `left_bound` is None
                    // `right_bound` is 1.0+ε
                    // `right_opposite_bound` is 1.0
                    if right_opposite_bound < right_bound {
                        return Compatibility::Incompatible(format!(
                            "[case 2,{index}] {self} does not contain {other}"
                        ));
                    }
                }
                (Some(left_bound), None, Some(right_opposite_bound)) => {
                    // This mirrors case 2.
                    if right_opposite_bound > left_bound {
                        return Compatibility::Incompatible(format!(
                            "[case 3,{index}] {self} does not contain {other}"
                        ));
                    }
                }
                (Some(_), None, _) => {
                    return Compatibility::Incompatible(format!(
                        "[case 4,{index}] {self} does not contain {other}"
                    ));
                }
                (Some(left_bound), Some(right_bound), _) => {
                    if left_bound > right_bound {
                        return Compatibility::Incompatible(format!(
                            "[case 5,{index}] {self} does not contain {other}"
                        ));
                    }
                }
            }
        }

        self.intersects(other)
    }

    fn get_compat_rule(&self) -> Option<CompatRule> {
        // Most types don't have a `CompatRule`
        None
    }

    fn intersects<R: Ranged>(&self, other: R) -> Compatibility {
        let mut self_valid_range = ValidRange::Total;
        let mut other_valid_range = ValidRange::Total;

        let self_lower = self.greater_or_equal_to();
        if let Some(greater_than_or_equal_to) = &self_lower {
            self_valid_range.restrict(std::ops::RangeFrom {
                start: greater_than_or_equal_to,
            });
        }

        let self_upper = self.less_than();
        if let Some(less_than) = &self_upper {
            self_valid_range.restrict(std::ops::RangeTo { end: less_than });
        }

        let other_lower = other.greater_or_equal_to();
        if let Some(greater_than_or_equal_to) = &other_lower {
            other_valid_range.restrict(std::ops::RangeFrom {
                start: greater_than_or_equal_to,
            });
        }

        let other_upper = other.less_than();
        if let Some(less_than) = &other_upper {
            other_valid_range.restrict(std::ops::RangeTo { end: less_than });
        }

        if self_valid_range.intersects(&other_valid_range) {
            Compatibility::Compatible
        } else {
            Compatibility::Incompatible(format!(
                "{} does not intersect with {}",
                self_valid_range, other_valid_range
            ))
        }
    }
}

impl<T: Ranged> Ranged for &T {
    fn contains<R: Ranged>(&self, other: R) -> Compatibility {
        Ranged::contains(*self, other)
    }
    fn get_compat_rule(&self) -> Option<CompatRule> {
        Ranged::get_compat_rule(*self)
    }
    fn greater_or_equal_to(&self) -> Option<Version> {
        Ranged::greater_or_equal_to(*self)
    }
    fn less_than(&self) -> Option<Version> {
        Ranged::less_than(*self)
    }
    fn intersects<R: Ranged>(&self, other: R) -> Compatibility {
        Ranged::intersects(*self, other)
    }
    fn is_applicable(&self, other: &Version) -> Compatibility {
        Ranged::is_applicable(*self, other)
    }
    fn is_satisfied_by<Spec>(&self, spec: &Spec, required: CompatRule) -> Compatibility
    where
        Spec: RecipeOps,
    {
        Ranged::is_satisfied_by(*self, spec, required)
    }
    fn rules(&self) -> BTreeSet<VersionRange> {
        Ranged::rules(*self)
    }
}

/// Specifies a range of version numbers by inclusion or exclusion
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[enum_dispatch(Ranged)]
pub enum VersionRange {
    Compat(CompatRange),
    DoubleEquals(DoubleEqualsVersion),
    DoubleNotEquals(DoubleNotEqualsVersion),
    Equals(EqualsVersion),
    Filter(VersionFilter),
    GreaterThan(GreaterThanRange),
    GreaterThanOrEqualTo(GreaterThanOrEqualToRange),
    LessThan(LessThanRange),
    LessThanOrEqualTo(LessThanOrEqualToRange),
    LowestSpecified(LowestSpecifiedRange),
    NotEquals(NotEqualsVersion),
    Semver(SemverRange),
    Wildcard(WildcardRange),
}

impl Display for VersionRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            VersionRange::Compat(vr) => vr.fmt(f),
            VersionRange::DoubleEquals(vr) => vr.fmt(f),
            VersionRange::DoubleNotEquals(vr) => vr.fmt(f),
            VersionRange::Equals(vr) => vr.fmt(f),
            VersionRange::Filter(vr) => vr.fmt(f),
            VersionRange::GreaterThan(vr) => vr.fmt(f),
            VersionRange::GreaterThanOrEqualTo(vr) => vr.fmt(f),
            VersionRange::LessThan(vr) => vr.fmt(f),
            VersionRange::LessThanOrEqualTo(vr) => vr.fmt(f),
            VersionRange::LowestSpecified(vr) => vr.fmt(f),
            VersionRange::NotEquals(vr) => vr.fmt(f),
            VersionRange::Semver(vr) => vr.fmt(f),
            VersionRange::Wildcard(vr) => vr.fmt(f),
        }
    }
}

impl IntoIterator for VersionRange {
    type Item = VersionRange;
    type IntoIter = std::collections::btree_set::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.rules().into_iter()
    }
}

impl std::str::FromStr for VersionRange {
    type Err = Error;

    fn from_str(rule_str: &str) -> Result<Self> {
        use nom::branch::alt;
        use nom::combinator::{all_consuming, eof, map};
        use nom::error::convert_error;

        all_consuming(alt((
            parsing::version_range,
            // Allow empty input to be treated like "*"
            map(
                eof,
                |_| VersionRange::Wildcard(WildcardRange::any_version()),
            ),
        )))(rule_str)
        .map(|(_, vr)| vr)
        .map_err(|err| match err {
            nom::Err::Error(e) | nom::Err::Failure(e) => Error::String(convert_error(rule_str, e)),
            nom::Err::Incomplete(_) => unreachable!(),
        })
    }
}

impl<T: Ranged> From<&T> for VersionRange {
    fn from(other: &T) -> Self {
        other.to_owned().into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct SemverRange {
    minimum: Version,
}

impl SemverRange {
    pub fn new(minimum: Version) -> Self {
        Self { minimum }
    }

    pub fn new_version_range<V: TryInto<Version, Error = crate::version::Error>>(
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
        Some(Version::from(parts))
    }
}

impl Display for SemverRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_char('^')?;
        f.write_str(&self.minimum.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct WildcardRange {
    specified: usize,
    parts: Vec<Option<u32>>,
}

impl WildcardRange {
    /// Return a `WildcardRange` representing "*".
    pub fn any_version() -> Self {
        Self {
            specified: 1,
            parts: vec![None],
        }
    }

    /// # Safety
    ///
    /// A `WildcardRange` must have one and only one optional part. This
    /// constructor does not verify this.
    pub unsafe fn new_unchecked(parts: Vec<Option<u32>>) -> Self {
        Self {
            specified: parts.len(),
            parts,
        }
    }

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
        // The placement of the wildcard dictates the floor version.
        //
        // *.2.3 -> >= 0.2.3
        // 1.*.3 -> >= 1.0.3
        // 1.2.* -> >= 1.2.0
        let parts = self
            .parts
            .clone()
            .into_iter()
            .map(|p| p.unwrap_or(0))
            .collect_vec();
        Some(Version::from_parts(parts.into_iter()))
    }

    fn less_than(&self) -> Option<Version> {
        // The placement of the wildcard dictates the ceiling version.
        //
        // *.2.3 -> [no limit]
        // 1.*.3 -> <2.0
        // 1.2.* -> <1.3
        let mut parts = self.parts.iter().peekable();
        let mut new_parts = Vec::with_capacity(self.parts.len());
        while let Some(x) = parts.next() {
            match (x, parts.peek()) {
                (None, None) => break,
                (None, Some(_)) => {
                    // Currently on the wildcard and there are more items;
                    // make this element a 0 and stop.
                    // This is like the second case in the example above.
                    new_parts.push(0);
                    break;
                }
                (Some(element_before_wildcard), Some(None)) => {
                    // The next element is the wildcard; increase the
                    // current element by one. This is like the second
                    // and third case in the example above.
                    new_parts.push(*element_before_wildcard + 1);
                }
                (Some(x), _) => {
                    // Haven't found the wildcard yet.
                    new_parts.push(*x);
                }
            }
        }
        if new_parts.is_empty() {
            None
        } else {
            Some(Version::from_parts(new_parts.into_iter()))
        }
    }

    fn is_applicable(&self, version: &Version) -> Compatibility {
        for (i, (a, b)) in self.parts.iter().zip(&*version.parts).enumerate() {
            if let Some(a) = a {
                if a != b {
                    return Compatibility::Incompatible(format!(
                        "Out of range: {self} [at pos {} ({}): has {b}, requires {a}]",
                        i + 1,
                        get_version_position_label(i),
                    ));
                }
            }
        }

        Compatibility::Compatible
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct LowestSpecifiedRange {
    specified: usize,
    base: Version,
}

impl LowestSpecifiedRange {
    pub const REQUIRED_NUMBER_OF_DIGITS: usize = 2;

    pub fn new(specified: usize, base: Version) -> Self {
        Self { specified, base }
    }
}

impl TryFrom<Version> for LowestSpecifiedRange {
    type Error = Error;

    fn try_from(base: Version) -> Result<Self> {
        let specified = base.parts.len();
        if specified < Self::REQUIRED_NUMBER_OF_DIGITS {
            Err(Error::String(format!(
                "Expected at least {required} digits in version range, got: {base}",
                required = Self::REQUIRED_NUMBER_OF_DIGITS
            )))
        } else {
            Ok(Self { specified, base })
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
}

impl Display for LowestSpecifiedRange {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let base_str = self.base.parts[..self.specified]
            .iter()
            .map(ToString::to_string)
            .collect_vec()
            .join(VERSION_SEP);
        f.write_char('~')?;
        f.write_str(&base_str)?;
        self.base.format_tags(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct GreaterThanRange {
    bound: Version,
}

impl GreaterThanRange {
    pub fn new(bound: Version) -> Self {
        Self { bound }
    }

    pub fn new_version_range<V: TryInto<Version, Error = crate::version::Error>>(
        boundary: V,
    ) -> Result<VersionRange> {
        Ok(VersionRange::GreaterThan(Self {
            bound: boundary.try_into()?,
        }))
    }
}

impl Ranged for GreaterThanRange {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.bound.clone().plus_epsilon())
    }

    fn less_than(&self) -> Option<Version> {
        None
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct LessThanRange {
    bound: Version,
}

impl LessThanRange {
    pub fn new(bound: Version) -> Self {
        Self { bound }
    }

    pub fn new_version_range<V: TryInto<Version, Error = crate::version::Error>>(
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct GreaterThanOrEqualToRange {
    bound: Version,
}

impl GreaterThanOrEqualToRange {
    pub fn new(bound: Version) -> Self {
        Self { bound }
    }

    pub fn new_version_range<V: TryInto<Version, Error = crate::version::Error>>(
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct LessThanOrEqualToRange {
    bound: Version,
}

impl LessThanOrEqualToRange {
    pub fn new(bound: Version) -> Self {
        Self { bound }
    }

    pub fn new_version_range<V: TryInto<Version, Error = crate::version::Error>>(
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
        Some(self.bound.clone().plus_epsilon())
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct EqualsVersion {
    version: Version,
}

impl EqualsVersion {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    pub fn version_range(version: Version) -> VersionRange {
        VersionRange::Equals(Self { version })
    }
}

impl From<Version> for EqualsVersion {
    fn from(version: Version) -> Self {
        Self { version }
    }
}

impl Ranged for EqualsVersion {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.version.clone())
    }

    fn less_than(&self) -> Option<Version> {
        Some(self.version.clone().plus_epsilon())
    }

    fn is_applicable(&self, other: &Version) -> Compatibility {
        if self.version.parts != other.parts {
            return Compatibility::Incompatible(format!("{} !! {} [not equal]", &other, self));
        }

        if self.version.pre != other.pre {
            return Compatibility::Incompatible(format!(
                "{other} !! {self} [not equal @ prerelease]",
            ));
        }
        // each post release tag must be exact if specified
        for (name, value) in self.version.post.iter() {
            if let Some(v) = other.post.get(name) {
                if v == value {
                    continue;
                }
            }
            return Compatibility::Incompatible(format!(
                "{other} !! {self} [not equal @ postrelease]",
            ));
        }
        Compatibility::Compatible
    }
}

impl Display for EqualsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_char('=')?;
        f.write_str(&self.version.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct NotEqualsVersion {
    specified: usize,
    base: Version,
}

impl NotEqualsVersion {
    pub fn new(specified: usize, base: Version) -> Self {
        Self { specified, base }
    }
}

impl From<Version> for NotEqualsVersion {
    fn from(base: Version) -> Self {
        let specified = base.parts.len();
        Self { specified, base }
    }
}

impl Ranged for NotEqualsVersion {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.base.clone().plus_epsilon())
    }

    fn less_than(&self) -> Option<Version> {
        Some(self.base.clone())
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
}

impl Display for NotEqualsVersion {
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
        f.write_str(&base_str)?;
        self.base.format_tags(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DoubleEqualsVersion {
    version: Version,
}

impl DoubleEqualsVersion {
    pub fn new(version: Version) -> Self {
        Self { version }
    }

    pub fn version_range(version: Version) -> VersionRange {
        VersionRange::DoubleEquals(Self { version })
    }
}

impl From<Version> for DoubleEqualsVersion {
    fn from(version: Version) -> Self {
        Self { version }
    }
}

impl Ranged for DoubleEqualsVersion {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.version.clone())
    }

    fn less_than(&self) -> Option<Version> {
        Some(self.version.clone().plus_epsilon())
    }

    fn is_applicable(&self, other: &Version) -> Compatibility {
        if self.version.parts != other.parts {
            return Compatibility::Incompatible(
                format!("{other} !! {self} [not equal precisely]",),
            );
        }

        if self.version.pre != other.pre {
            return Compatibility::Incompatible(format!(
                "{other} !! {self} [not equal precisely @ prerelease]",
            ));
        }
        // post release tags must match exactly
        if self.version.post != other.post {
            return Compatibility::Incompatible(format!(
                "{other} !! {self} [not equal precisely @ postrelease]",
            ));
        }
        Compatibility::Compatible
    }
}

impl Display for DoubleEqualsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("==")?;
        f.write_str(&self.version.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DoubleNotEqualsVersion {
    specified: usize,
    base: Version,
}

impl DoubleNotEqualsVersion {
    pub fn new(specified: usize, base: Version) -> Self {
        Self { specified, base }
    }
}

impl From<Version> for DoubleNotEqualsVersion {
    fn from(base: Version) -> Self {
        let specified = base.parts.len();
        Self { specified, base }
    }
}

impl Ranged for DoubleNotEqualsVersion {
    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.base.clone().plus_epsilon())
    }

    fn less_than(&self) -> Option<Version> {
        Some(self.base.clone())
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

        // To mirror `PreciseExactVersion`, any differences in post
        // releases makes these unequal.
        if self.base.post != version.post {
            return Compatibility::Compatible;
        }

        Compatibility::Incompatible(format!("excluded precisely [{}]", self))
    }
}

impl Display for DoubleNotEqualsVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let base_str = self
            .base
            .parts
            .iter()
            .take(self.specified)
            .map(ToString::to_string)
            .collect_vec()
            .join(VERSION_SEP);
        f.write_str("!==")?;
        f.write_str(&base_str)?;
        self.base.format_tags(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct CompatRange {
    base: Version,
    /// if unset, the required compatibility is based on the type
    /// of package being validated. Source packages require api
    /// compat and binary packages require binary compat.
    required: Option<CompatRule>,
}

impl CompatRange {
    pub fn new(base: Version, required: Option<CompatRule>) -> Self {
        Self { base, required }
    }

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
    fn get_compat_rule(&self) -> Option<CompatRule> {
        self.required
    }

    fn greater_or_equal_to(&self) -> Option<Version> {
        Some(self.base.clone())
    }

    fn less_than(&self) -> Option<Version> {
        None
    }

    fn is_satisfied_by<Spec>(&self, spec: &Spec, mut required: CompatRule) -> Compatibility
    where
        Spec: RecipeOps,
    {
        // XXX: Should this custom logic be in `is_applicable` instead?
        if let Some(r) = self.required {
            required = r;
        }
        match required {
            CompatRule::None => Compatibility::Compatible,
            CompatRule::API => spec.is_api_compatible(&self.base),
            CompatRule::Binary => spec.is_binary_compatible(&self.base),
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

/// Control how [`VersionFilter::restrict`] will handle
/// two version ranges that do not intersect.
#[derive(Debug)]
pub enum RestrictMode {
    /// If the two ranges do not intersect, an attempt to restrict them will
    /// fail.
    RequireIntersectingRanges,
    /// The two ranges are not required to intersect. If they do not, the
    /// two ranges are concatenated and the resulting version range will have
    /// no versions that can satisfy it.
    AllowNonIntersectingRanges,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct VersionFilter {
    // Use `BTreeSet` to make `to_string` output consistent.
    rules: BTreeSet<VersionRange>,
}

impl VersionFilter {
    pub fn new<I: IntoIterator<Item = VersionRange>>(rules: I) -> Self {
        Self {
            rules: rules.into_iter().collect(),
        }
    }

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

    /// Flatten this filter's rules to remove nested `VersionFilter`.
    pub fn flatten(self) -> Self {
        VersionFilter {
            rules: self
                .rules
                .into_iter()
                .flat_map(|r| match r {
                    VersionRange::Filter(f) => VersionRange::Filter(f.flatten()),
                    _ => r,
                })
                .collect(),
        }
    }

    /// Reduce this range by the scope of another
    ///
    /// This version range will become restricted to the intersection
    /// of the current version range and the other. Or, if they do not
    /// intersect, then `mode` determines if an error is returned or if the
    /// two ranges will be concatenated without any simplification.
    pub fn restrict(&mut self, other: impl Ranged, mode: RestrictMode) -> Result<()> {
        let compat = self.intersects(&other);
        if let Compatibility::Incompatible(msg) = compat {
            if matches!(mode, RestrictMode::AllowNonIntersectingRanges) {
                self.rules.extend(other.rules());
                return Ok(());
            }

            return Err(Error::String(msg));
        }

        // Combine the two rule sets and then simplify them.
        self.rules.extend(other.rules());
        // Do not merge any `CompatRange` since this can lose a
        // rule for a smaller version number, as in:
        //     maya/2019,maya/2020
        // It is unknown what the `compat` values will be for any
        // given build of maya, and it is not safe to simplify
        // this request to just "maya/2020".
        self.simplify_rules(false);

        Ok(())
    }

    /// Remove redundant rules from a set of `VersionRange` values.
    fn simplify_rules(&mut self, allow_compat_ranges_to_merge: bool) {
        if self.rules.len() <= 1 {
            return;
        }

        // Simply the rules, e.g., turn ">1.0,>2.0" into ">2.0".
        let mut rules_as_vec = std::mem::take(&mut self.rules)
            .into_iter()
            .collect::<Vec<_>>();
        let mut index_to_remove = None;
        'start_over: loop {
            if let Some(index) = index_to_remove.take() {
                rules_as_vec.remove(index);
            }

            for candidates in rules_as_vec.iter().enumerate().permutations(2) {
                let (lhs_index, lhs_vr) = candidates.get(0).unwrap();
                let (_, rhs_vr) = candidates.get(1).unwrap();

                // `Binary:1.2.3` and `=1.2.3` are allowed to merge as a
                // special case.
                if !allow_compat_ranges_to_merge {
                    match (lhs_vr, rhs_vr) {
                        (VersionRange::Compat(lhs), VersionRange::Equals(rhs))
                            if lhs.base == rhs.version => {}
                        (VersionRange::Compat(lhs), VersionRange::DoubleEquals(rhs))
                            if lhs.base == rhs.version => {}
                        (_, VersionRange::Compat(_)) | (VersionRange::Compat(_), _) => continue,
                        _ => {}
                    };
                }

                // Note that `permutations` will give every element a chance
                // to appear on the lhs. We don't have to check in both
                // directions in here.
                if lhs_vr.contains(rhs_vr).is_ok() {
                    // Keep the more restrictive rule and restart comparing.
                    // `rules_as_vec` is borrowed immutably here.
                    index_to_remove = Some(*lhs_index);
                    continue 'start_over;
                }
            }
            break;
        }
        self.rules = rules_as_vec.into_iter().collect();
    }

    /// Convert this version filter to a plain [`Version`], if possible.
    ///
    /// `1.2.3`, `=1.2.3`, `==1.2.3` can convert to `1.2.3`.
    pub fn try_into_version(self) -> Result<Version> {
        // dev note: this could be a method on `Ranged` but it wants to
        // consume the `VersionFilter`.
        let mut rules = self.rules.into_iter();
        let rule = rules
            .next()
            .ok_or_else(|| Error::String("VersionFilter cannot be empty".to_owned()))?;
        if rules.next().is_some() {
            return Err("VersionFilter must have exactly one rule".into());
        }

        Ok(match rule {
            VersionRange::Compat(v) => v.base,
            VersionRange::Equals(v) => v.version,
            VersionRange::DoubleEquals(v) => v.version,
            VersionRange::Filter(f) => return f.try_into_version(),
            _ => return Err(format!("'{rule}' cannot be expressed as a `Version`").into()),
        })
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
    fn is_satisfied_by<Spec>(&self, spec: &Spec, required: CompatRule) -> Compatibility
    where
        Spec: RecipeOps,
    {
        // XXX: Could this custom logic be handled by `is_applicable` instead?
        for rule in self.rules.iter() {
            let compat = rule.is_satisfied_by(spec, required);
            if !compat.is_ok() {
                return compat;
            }
        }

        Compatibility::Compatible
    }

    fn contains<R: Ranged>(&self, other: R) -> Compatibility {
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

    fn intersects<R: Ranged>(&self, other: R) -> Compatibility {
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

    fn rules(&self) -> BTreeSet<VersionRange> {
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

impl From<Version> for VersionFilter {
    fn from(version: Version) -> Self {
        Self::single(VersionRange::DoubleEquals(DoubleEqualsVersion { version }))
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
            if rule_str.is_empty() {
                return Err(Error::String(format!(
                    "Empty segment not allowed in version range, got: {}",
                    range
                )));
            }
            let rule = VersionRange::from_str(rule_str)?;
            out.rules.insert(rule);
        }

        Ok(out)
    }
}

pub fn parse_version_range<S: AsRef<str>>(range: S) -> Result<VersionRange> {
    let mut filter = VersionFilter::from_str(range.as_ref())?;

    // Two or more `CompatRange` rules in a single request are
    // eligible to be merged. At least, no use case for preserving
    // the unmerged requests is currently known.
    filter.simplify_rules(true);

    if filter.rules.len() == 1 {
        Ok(filter.rules.into_iter().next().unwrap())
    } else {
        Ok(VersionRange::Filter(filter))
    }
}
