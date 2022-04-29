// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::{char, digit1},
    combinator::{cut, map, map_parser, map_res, rest},
    error::{context, VerboseError},
    multi::separated_list0,
    sequence::preceded,
    FindToken, IResult,
};

use crate::api::{
    CompatRange, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, SemverRange, VersionRange, WildcardRange,
};

use super::version::{version, version_str};

pub(crate) fn wildcard_range(
    require_star: bool,
    fail_if_contains_star: bool,
) -> impl Fn(&str) -> IResult<&str, VersionRange, VerboseError<&str>> {
    move |input| {
        let mut parser = map_res(
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
        );
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

pub(crate) fn version_range(input: &str) -> IResult<&str, VersionRange, VerboseError<&str>> {
    context(
        "version_range",
        alt((
            // Use `cut` for these that first match on an operator first,
            // if the version fails to parse then it shouldn't continue to
            // try the other options of the `alt` here.
            map_res(
                preceded(char('^'), cut(version_str)),
                SemverRange::new_version_range,
            ),
            map_res(
                preceded(char('~'), cut(version_str)),
                LowestSpecifiedRange::new_version_range,
            ),
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
            map_res(
                preceded(tag("!=="), cut(version_str)),
                DoubleNotEqualsVersion::new_version_range,
            ),
            map_res(
                preceded(tag("!="), cut(version_str)),
                NotEqualsVersion::new_version_range,
            ),
            map_parser(
                is_not(",/"),
                alt((
                    wildcard_range(true, true),
                    context(
                        "CompatRange::new_version_range",
                        map_res(rest, CompatRange::new_version_range),
                    ),
                )),
            ),
        )),
    )(input)
}
