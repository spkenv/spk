// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, one_of},
    combinator::{all_consuming, cut, eof, map, map_parser, map_res, recognize, rest},
    error::{context, VerboseError},
    multi::{many1, separated_list0, separated_list1},
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

pub(crate) fn wildcard_range(
    require_star: bool,
    fail_if_contains_star: bool,
) -> impl Fn(&str) -> IResult<&str, VersionRange, VerboseError<&str>> {
    move |input| {
        let mut parser = all_consuming(map_res(
            separated_list0(
                tag("."),
                alt((
                    map_res(recognize(many1(one_of("0123456789"))), |n: &str| {
                        n.parse::<u32>().map(Some)
                    }),
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

pub(crate) fn version_range(
    require_star: bool,
    fail_if_contains_star: bool,
) -> impl Fn(&str) -> IResult<&str, VersionRange, VerboseError<&str>> {
    move |input: &str| {
        context(
            "version_range",
            map(
                separated_list1(
                    tag(crate::api::VERSION_RANGE_SEP),
                    map_parser(
                        alt((is_not(crate::api::VERSION_RANGE_SEP), eof)),
                        alt((
                            // Use `cut` for these that first match on an operator first,
                            // if the version fails to parse then it shouldn't continue to
                            // try the other options of the `alt` here.
                            map_res(preceded(char('^'), cut(version_str)), |s| {
                                SemverRange::new_version_range(s)
                            }),
                            map_res(preceded(char('~'), cut(version_str)), |s| {
                                LowestSpecifiedRange::new_version_range(s)
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
                            map_res(preceded(tag("!=="), cut(version_str)), |s| {
                                DoubleNotEqualsVersion::new_version_range(s)
                            }),
                            map_res(preceded(tag("!="), cut(version_str)), |s| {
                                NotEqualsVersion::new_version_range(s)
                            }),
                            alt((
                                wildcard_range(require_star, fail_if_contains_star),
                                context(
                                    "CompatRange::new_version_range",
                                    map_res(rest, CompatRange::new_version_range),
                                ),
                            )),
                        )),
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
