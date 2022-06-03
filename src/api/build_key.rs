// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use super::{parse_version_range, Ident, OptionMap, Ranged, Version};

use crate::Result;

#[cfg(test)]
#[path = "./build_key_test.rs"]
mod build_key_test;

// This module defines the structures that make up compound build keys
// used in the 'by_build_option_values' build sorting method.
//
// Note: If you are looking at this for the first time, start with the
// enum BuildKey and its docs at near then end of this file and work
// backwards.

/// One piece in a list of a pieces for an expanded version number
/// value entry in a build key. The order these are defined in
/// will put Numbers behind Text values in a normal build ordering
/// (reverse sort).
#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
enum BuildKeyVersionNumberPiece {
    /// For number parts, e.g. the 2 or 3 or 1, in 2.3.1
    Number(u32),
    /// For tag names parts, e.g. the "r" in in 2.3.1+r.2
    Text(String),
}

/// A fully expanded version number with all its pieces for use in
/// a build key, e.g. 6.4.0+r2 effectively becomes:
/// { digits: [6, 4, 0], posttag: Some(['r', 2]), pretag: None }
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct BuildKeyVersionNumber {
    /// The major, minor, patch, and tail digits, e.g. [6, 4, 0]
    digits: Vec<BuildKeyVersionNumberPiece>,
    /// Any post-release tag pieces, e.g. Some(['r', 1]) or None
    posttag: Option<Vec<BuildKeyVersionNumberPiece>>,
    /// Any pre-release tag pieces, e.g. Some(['r', 2]) or None
    pretag: Option<Vec<BuildKeyVersionNumberPiece>>,
}

impl BuildKeyVersionNumber {
    /// This takes a version number, separates it into pieces and turns
    /// them into a BuildKeyVersionNumber suitable for comparing as a
    /// component of a build key,
    /// e.g.
    ///      2.3.4-r.1 => { digits: [2,3,4], posttag: None, pretag: Some(['r', 1]) }
    ///      2.3.4     => { digits: [2,3,4], posttag: None, pretag: None }
    ///      2.3.4+r.2 => { digits: [2,3,4], posttag: Some(['r', 2]), pretag: None }
    pub(crate) fn new(v: &Version) -> Self {
        // Collect the version's number parts into a form suitable for use
        // in a build key.
        let digits: Vec<BuildKeyVersionNumberPiece> = v
            .parts
            .iter()
            .map(|n| BuildKeyVersionNumberPiece::Number(*n))
            .collect();

        // Add post tag pieces. Versions without a posttag get an empty value
        let posttag = if v.post.is_empty() {
            None
        } else {
            // There can be multiple post tags in a Version. There usually
            // aren't but this copes with them.
            let mut posttags: Vec<BuildKeyVersionNumberPiece> = Vec::new();
            for (name, value) in v.post.get_tags() {
                posttags.push(BuildKeyVersionNumberPiece::Text(name.to_string()));
                posttags.push(BuildKeyVersionNumberPiece::Number(*value));
            }
            Some(posttags)
        };

        // Add pre tags piece. Versions without a pretag get an empty value
        let pretag = if v.pre.is_empty() {
            None
        } else {
            // There can be multiple pre tags in a Version. There usually
            // aren't but this copes with them.
            let mut pretags: Vec<BuildKeyVersionNumberPiece> = Vec::new();
            for (name, value) in v.pre.get_tags() {
                pretags.push(BuildKeyVersionNumberPiece::Text(name.to_string()));
                pretags.push(BuildKeyVersionNumberPiece::Number(*value));
            }
            Some(pretags)
        };

        // Combine the pieces in a form suitable for sorting. Digits are
        // first as the most important, then post tags, with pre tags last
        BuildKeyVersionNumber {
            digits,
            posttag,
            pretag,
        }
    }
}

/// An expanded version range value consisting of the max and min
/// values it allows and a comparison tie-breaker for use when two
/// expanded version ranges have the same max and min values.
///
/// e.g. ~6.3.1 effectively becomes:
///   {
///     max: { digits: [6, 4, 0], posttag: None, pretag: None },
///     min: { digits: [6, 3, 1], posttag: None, pretag: None },
///     tie-breaker: 18385578307071417374
///   }
///
/// BuildKeyExpandedVersionRange is designed to be used a component in
/// a build ordering key. The max version is first and the min version
/// second, so the max will take priority over the min when used in an
/// ordering between BuildKeyExpandedVersionRanges.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct BuildKeyExpandedVersionRange {
    /// The maximum version number limit expanded out into pieces,
    /// e.g.  { digits: [6, 4, 0], posttag: None, pretag: None }
    max: BuildKeyVersionNumber,
    /// The minimum version number expanded, e.g.
    /// { digits: [6, 3, 1], posttag: None, pretag: None }
    min: BuildKeyVersionNumber,
    /// A hash of the original version request string for breaking ties,
    /// e.g. 18385578307071417374
    tie_breaker: u64,
}

impl BuildKeyExpandedVersionRange {
    /// Generates a number that can be used as a tie-breaker in cases
    /// where two version range values turn out to have the same max
    /// and min version limits.
    pub(crate) fn generate_tie_breaker(value: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }

    /// Parses a version request string into a version filter, uses
    /// that to work out the max and min version number limits for
    /// that version request (min <= version < max), then returns a
    /// BuildKeyExpandedVersionRange representation of that version
    /// request. The Errors if it cannot parse the given string as a
    /// version range request.
    ///
    /// e.g.
    ///  "~2.3.4-r.1" effectively becomes
    ///      {
    ///        max: { digits: [2, 3, 5], posttag: None, pretag: None },
    ///        min: { digits: [2, 3, 5], posttag: None, pretag: Some(['r', 1]) },
    ///        tie-breaker: 13927370486250613811
    ///      }
    ///  "~2.3.4" effectively becomes
    ///      {
    ///        max: { digits: [2, 4, 0], posttag: None, pretag: None },
    ///        min: { digits: [2, 3, 4], posttag: None, pretag: None },
    ///        tie-breaker: 7318170121295493534
    ///      }
    ///  "~2.3.4+r.2" effectively becomes
    ///      {
    ///        max: { digits: [2, 3, 5], posttag: None, pretag: None },
    ///        min: { digits: [2, 3, 4], posttag: Some(['r', 2]), pretag: None },
    ///        tie-breaker: 6310623536257608547
    ///      }
    pub(crate) fn parse_from_range_value<S: AsRef<str>>(
        range: S,
    ) -> Result<BuildKeyExpandedVersionRange> {
        // Turn the version request string into a version filter. If this
        // fails, then this function cannot continue. The max and min
        // bounds can only be obtained from a valid version filter.
        let filter = parse_version_range(range.as_ref())?;

        // Max version limit: version < max
        let max: BuildKeyVersionNumber = match filter.less_than() {
            Some(v) => BuildKeyVersionNumber::new(&v),
            None => {
                // This happens when there is no max, so the sky's the
                // limit for this version filter. For example: ">=1.2.3".
                // But an empty value is last in a reverse sort and that's
                // a problem if the max really is unlimited, so use a
                // Version based on the maximum possible numbers instead.
                BuildKeyVersionNumber::new(&Version::new(u32::MAX, u32::MAX, u32::MAX))
            }
        };

        // Min allowed version: min <= version
        let min: BuildKeyVersionNumber = match filter.greater_or_equal_to() {
            Some(v) => BuildKeyVersionNumber::new(&v),
            None => {
                // This happens when there is no min, so the ground's the
                // limit. For example: "<1.2.3". An empty value is last in
                // a reverse sort which is fine, but to make it consistent
                // with other values a Version based on the minimum
                // numbers is used instead.
                BuildKeyVersionNumber::new(&Version::new(0, 0, 0))
            }
        };

        // In case two version requests translate to the same max and min
        // values, a hash of the original string value is generated as a
        // possible tie-breaker. One example of this is '1.2.3' and
        // '>=1.2.3', both have the same max and min values and need the
        // tie-breaker to be consistently ordered.
        let tie_breaker = BuildKeyExpandedVersionRange::generate_tie_breaker(range.as_ref());

        // Max version, min version, tie-breaker - in that order because
        // the max should take priority over the min when used in an
        // ordering key.
        Ok(BuildKeyExpandedVersionRange {
            max,
            min,
            tie_breaker,
        })
    }
}

/// A single value component of a build key. When there is no value,
/// NotSet is used. When the value parses as a version request,
/// ExpandedVersion is used. Text is used for all other kinds of
/// values.
///
/// They are defined in the order below to ensure that when reverse
/// sorted ExpandedVersions come before Text values, which come before
/// NotSet values. This has the side-effect of putting builds with
/// more dependencies before those with fewer dependencies (which will
/// have more NotSet values).
// TODO: should builds with more dependencies be preferred over ones
// with fewer dependencies? I think I'd rather have ones with fewer
// dependencies first, maybe?
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildKeyEntry {
    /// This value is not set because the build option did not have a
    /// value for the name this entry is generated from. This can
    /// happen when one build has different options set than another
    /// build.
    NotSet,
    /// This value is a string value (that did not parse as a version
    /// request number or range, see next entry), e.g. 'cp37m' or 'on'
    Text(String),
    /// This value is (was parsed as) a version request number. This
    /// is used when the value was successfully expanded into a
    /// version range build key value, e.g. 6.3.1 or ~1.2.3
    ExpandedVersion(BuildKeyExpandedVersionRange),
}

/// A BuildKey is for ordering builds within a package version. There
/// are 2 kinds of BuildKey: a simple key for /src builds, and a
/// compound key for non-src builds. /src package builds are always
/// put last in a reverse sort, and are all considered equal, so their
/// keys don't contain any detailed information. Non-src builds'
/// compound keys are made up of multiple components designed to help
/// order the builds so that:
///
/// - Builds are ordered based on the values of their build options
///
/// - The build option to consider first is based on an ordering of a
///   subset of build option names by importance, with the remaining
///   build options being considered in alphabetical name order
///
/// - If a build option value is a version request, it is converted
///   into an expanded version range with max and min version number
///   bounds, such that: min <= version < max.
///
/// - If a value cannot be converted to an expanded version range,
///   it is left as is (a Text string)
///
/// - If a build does not have a value for a build option, it is
///   given a NotSet value
///
/// - Because builds are reverse sorted, within a value:
///   - ExpandedVersion range values are ordered first, the ones
///     with higher maximums will be first, then those with highest
///     minimums. This puts highest version numbers ahead of lower
///     ones within a build option
///   - Text values are ordered next, by reverse alphabetical ordering
///     of the strings. This puts values like "on" before "off"
///   - NotSet values are ordered last, they are all equal.
///
/// A full build key might look something like this, e.g. if the
/// values for the first 3 option names were: '>2.0.0', no value for
/// the second option, and 'apples' was the value for the third
/// option:
///
/// NonSrc(
///   [
///     ExpandedVersion(
///       {
///         max: {
///                 digits: [4294967295, 4294967295, 4294967295],
///                 posttag: None,
///                 pretag: None
///              },
///         min: {
///                digits:[2, 0, 0],
///                posttag: None,
///                pretag: None
///              }
///         tie_breaker: 16007767698936169634
///       }
///     ),
///     NotSet,
///     Text('apples'),
///     ...
///   ]
/// )
///
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum BuildKey {
    /// A /src build key, it contains no extra information. In a
    /// reverse sort with non-src builds /src builds are always placed
    /// last among sorted builds.
    Src,
    /// A non-src build key. These build's keys are an ordered list of
    /// key entry components.
    NonSrc(Vec<BuildKeyEntry>),
}

impl BuildKey {
    /// This makes a new compound multi-entry build key for a build
    /// based on the given ordering of option names, and resolved name
    /// values.
    ///
    /// Note: This assumes the given name_values OptionMap are correct
    /// for the matching build (pkg Ident). If not, it will make a
    /// strange build key that could be unrelated to the build. See
    /// SortedBuildIterator for more details.
    pub fn new(pkg: &Ident, ordering: &Vec<String>, name_values: &OptionMap) -> BuildKey {
        // Make the build key based on '/src' or non-/src, the given
        // ordering of option names, and the given values for those names
        if pkg.is_source() {
            // All '/src' build use the same simplified key
            return BuildKey::Src;
        }

        // This is a non-/src build. Construct a compound key from the
        // ordering of option names and the given values for those names.
        let mut key_entries: Vec<BuildKeyEntry> = Vec::with_capacity(ordering.len());
        for name in ordering {
            // Generate this entry based on the value set for this name
            let entry: BuildKeyEntry = match name_values.get(name) {
                Some(value) => match BuildKeyExpandedVersionRange::parse_from_range_value(value) {
                    Ok(expanded_version) => BuildKeyEntry::ExpandedVersion(expanded_version),
                    Err(_) => BuildKeyEntry::Text(value.clone()),
                },
                None => BuildKeyEntry::NotSet,
            };
            key_entries.push(entry);
        }

        // Assemble and return the build key
        BuildKey::NonSrc(key_entries)
    }
}
