// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryFrom;

use nom::{
    branch::alt,
    character::complete::{char, one_of},
    combinator::{cut, map, map_res, recognize, verify},
    error::{context, ContextError, FromExternalError, ParseError},
    multi::{many1, separated_list1},
    sequence::{pair, preceded, terminated},
    IResult,
};
use nom_supreme::tag::{complete::tag, TagError};

use crate::api::{
    CompatRange, CompatRule, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, SemverRange, VersionFilter, VersionRange,
    WildcardRange,
};

use super::version::{version, version_str};

/// Parse a compat range into a [`VersionRange`].
///
/// A compat range is a plain version number preceded by a compatibility
/// requirement.
///
/// Examples:
/// - Binary:1.0
/// - API:1.0
pub(crate) fn compat_range<'a, E>(input: &'a str) -> IResult<&'a str, VersionRange, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    map(
        pair(terminated(compat_rule, char(':')), cut(version)),
        |(required, base)| {
            VersionRange::Compat(CompatRange {
                base,
                required: Some(required),
            })
        },
    )(input)
}

/// Parse a [`CompatRule'].
///
/// This is either the word "Binary" or "API".
pub(crate) fn compat_rule<'a, E>(input: &'a str) -> IResult<&'a str, CompatRule, E>
where
    E: ParseError<&'a str> + TagError<&'a str, &'static str>,
{
    alt((
        map(tag("Binary"), |_| CompatRule::Binary),
        map(tag("API"), |_| CompatRule::API),
    ))(input)
}

/// Parse a wildcard range into a [`VersionRange`].
///
/// One wildcard is required.
///
/// Examples:
/// - `"*"`
/// - `"1.*"`
/// - `"*.1"`
pub(crate) fn wildcard_range<'a, E>(input: &'a str) -> IResult<&'a str, VersionRange, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    map(
        verify(
            separated_list1(
                tag("."),
                alt((
                    map_res(recognize(many1(one_of("0123456789"))), |n: &str| {
                        n.parse::<u32>().map(Some)
                    }),
                    map(tag("*"), |_| None),
                )),
            ),
            |parts: &Vec<Option<u32>>| parts.iter().filter(|p| p.is_none()).count() == 1,
        ),
        |parts| {
            VersionRange::Wildcard(WildcardRange {
                specified: parts.len(),
                parts,
            })
        },
    )(input)
}

/// Parse a version filter into a [`VersionRange`].
///
/// A version filter is either a single expression or a comma-separated
/// list of expressions.
///
/// Examples:
/// - `"!=1.0"`
/// - `"!==1.0"`
/// - `"1.*"`
/// - `"1.0"`
/// - `"<1.0"`
/// - `"<=1.0"`
/// - `"=1.0"`
/// - `"==1.0"`
/// - `">1.0"`
/// - `">=1.0"`
/// - `"^1.0"`
/// - `"~1.0"`
/// - `">1.0,<2.0"`
pub(crate) fn version_range<'a, E>(input: &'a str) -> IResult<&'a str, VersionRange, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    context(
        "version_range",
        map(
            separated_list1(
                tag(crate::api::VERSION_RANGE_SEP),
                alt((
                    // Use `cut` for these that first match on an operator first,
                    // if the version fails to parse then it shouldn't continue to
                    // try the other options of the `alt` here.
                    map_res(preceded(char('^'), cut(version_str)), |s| {
                        SemverRange::new_version_range(s)
                    }),
                    map_res(preceded(char('~'), cut(version)), |v| {
                        LowestSpecifiedRange::try_from(v).map(VersionRange::LowestSpecified)
                    }),
                    map_res(preceded(tag(">="), cut(version_str)), |s| {
                        GreaterThanOrEqualToRange::new_version_range(s)
                    }),
                    map_res(preceded(tag("<="), cut(version_str)), |s| {
                        LessThanOrEqualToRange::new_version_range(s)
                    }),
                    map_res(preceded(char('>'), cut(version_str)), |s| {
                        GreaterThanRange::new_version_range(s)
                    }),
                    map_res(preceded(char('<'), cut(version_str)), |s| {
                        LessThanRange::new_version_range(s)
                    }),
                    map(preceded(tag("=="), cut(version)), |v| {
                        DoubleEqualsVersion::version_range(v)
                    }),
                    map(preceded(char('='), cut(version)), |v| {
                        EqualsVersion::version_range(v)
                    }),
                    map(preceded(tag("!=="), cut(version)), |v| {
                        VersionRange::DoubleNotEquals(DoubleNotEqualsVersion::from(v))
                    }),
                    map(preceded(tag("!="), cut(version)), |v| {
                        VersionRange::NotEquals(NotEqualsVersion::from(v))
                    }),
                    compat_range,
                    wildcard_range,
                    // Just a plain version can be a version range.
                    map(version, |base| {
                        VersionRange::Compat(CompatRange {
                            base,
                            required: None,
                        })
                    }),
                )),
            ),
            |mut version_range| {
                if version_range.len() == 1 {
                    version_range.remove(0)
                } else {
                    VersionRange::Filter(VersionFilter {
                        rules: version_range.into_iter().collect(),
                    })
                }
            },
        ),
    )(input)
}
