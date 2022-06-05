// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryFrom;

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, digit1},
    combinator::{all_consuming, cut, eof, map, map_parser, map_res, rest},
    error::{context, ContextError, FromExternalError, ParseError},
    multi::{separated_list0, separated_list1},
    sequence::preceded,
    FindToken, IResult,
};

use crate::api::{
    CompatRange, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, SemverRange, VersionFilter, VersionRange,
    WildcardRange,
};

use super::version::{version, version_str};

/// Parse a wildcard range into a [`VersionRange`].
///
/// If `require_star` is true, the input is only considered valid
/// if it contains a `*` character. Otherwise, an empty string
/// is a valid input and is interpreted as if the input was `"*"`.
///
/// If `fail_if_contains_star` is true, the parser will terminate
/// with an "unrecoverable" failure instead of a normal error if
/// the input contains a `*` character.
///
/// At most one wildcard is allowed.
///
/// Examples:
/// - `""`
/// - `"*"`
/// - `"1.*"`
/// - `"*.1"`
pub(crate) fn wildcard_range<'a, E>(
    require_star: bool,
    fail_if_contains_star: bool,
) -> impl Fn(&'a str) -> IResult<&'a str, VersionRange, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    move |input| {
        let mut parser = all_consuming(map_res(
            separated_list0(
                tag("."),
                alt((
                    map_res(digit1, |n: &str| n.parse::<u32>().map(Some)),
                    map(tag("*"), |_| None),
                )),
            ),
            |parts| {
                if parts.is_empty() && !require_star {
                    WildcardRange::new_version_range("*")
                } else if parts.iter().filter(|p| p.is_none()).count() != 1 {
                    Err(crate::Error::String(format!(
                        "Expected exactly one wildcard in version range, got: {input}"
                    )))
                } else {
                    Ok(VersionRange::Wildcard(WildcardRange {
                        specified: parts.len(),
                        parts,
                    }))
                }
            },
        ));
        if fail_if_contains_star && input.find_token('*') {
            // This `cut` is so if the input contains '*' but parsing
            // fails, this becomes a hard error instead of trying other
            // alternates (in the outer scope).
            cut(parser)(input)
        } else {
            parser(input)
        }
    }
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
///
/// See [wildcard_range] for an explanation of `require_star` and
/// `fail_if_contains_star`.
pub(crate) fn version_range<'a, E>(
    require_star: bool,
    fail_if_contains_star: bool,
) -> impl Fn(&'a str) -> IResult<&'a str, VersionRange, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    move |input: &str| {
        context(
            "version_range",
            map(
                separated_list1(
                    tag(crate::api::VERSION_RANGE_SEP),
                    map_parser(
                        alt((is_not(crate::api::VERSION_RANGE_SEP), eof)),
                        all_consuming(alt((
                            // Use `cut` for these that first match on an operator first,
                            // if the version fails to parse then it shouldn't continue to
                            // try the other options of the `alt` here.
                            map_res(
                                preceded(char('^'), cut(version_str)),
                                SemverRange::new_version_range,
                            ),
                            map_res(preceded(char('~'), cut(version)), |v| {
                                LowestSpecifiedRange::try_from(v).map(VersionRange::LowestSpecified)
                            }),
                            map_res(
                                preceded(tag(">="), cut(version_str)),
                                GreaterThanOrEqualToRange::new_version_range,
                            ),
                            map_res(
                                preceded(tag("<="), cut(version_str)),
                                LessThanOrEqualToRange::new_version_range,
                            ),
                            map_res(
                                preceded(char('>'), cut(version_str)),
                                GreaterThanRange::new_version_range,
                            ),
                            map_res(
                                preceded(char('<'), cut(version_str)),
                                LessThanRange::new_version_range,
                            ),
                            map(
                                preceded(tag("=="), cut(version)),
                                DoubleEqualsVersion::version_range,
                            ),
                            map(
                                preceded(char('='), cut(version)),
                                EqualsVersion::version_range,
                            ),
                            map(preceded(tag("!=="), cut(version)), |v| {
                                VersionRange::DoubleNotEquals(DoubleNotEqualsVersion::from(v))
                            }),
                            map(preceded(tag("!="), cut(version)), |v| {
                                VersionRange::NotEquals(NotEqualsVersion::from(v))
                            }),
                            alt((
                                wildcard_range(require_star, fail_if_contains_star),
                                context(
                                    "CompatRange::new_version_range",
                                    map_res(rest, CompatRange::new_version_range),
                                ),
                            )),
                        ))),
                    ),
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
}
