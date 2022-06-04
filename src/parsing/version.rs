// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use nom::{
    character::complete::{char, digit1},
    combinator::{map_res, opt, recognize},
    error::{context, ContextError, FromExternalError, ParseError},
    multi::separated_list1,
    sequence::{pair, preceded, separated_pair},
    IResult,
};

use crate::api::{parse_version, Version};

use super::name::tag_name;

/// Parse a valid version pre- or post-tag.
///
/// A valid tag is comprised of a string and a number, separated
/// by a `.`.
///
/// Examples:
/// - `"r.0"`
/// - `"alpha1.400"`
pub(crate) fn ptag<'a, E>(input: &'a str) -> IResult<&'a str, (&'a str, &'a str), E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    separated_pair(tag_name, char('.'), digit1)(input)
}

/// Parse a valid pre- or post-tag set.
///
/// A tag set is a comma-separated list of [`ptag`].
///
/// Example: `"r.0,alpha1.400"`
pub(crate) fn ptagset<'a, E>(input: &'a str) -> IResult<&'a str, Vec<(&'a str, &'a str)>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    separated_list1(char(','), ptag)(input)
}

/// Parse a version string into a [`Version`].
///
/// See [version_str] for examples of valid version strings.
pub(crate) fn version<'a, E>(input: &'a str) -> IResult<&'a str, Version, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>,
{
    map_res(version_str, parse_version)(input)
}

/// Parse a version.
///
/// A version is a version number followed by optional
/// pre-release tags and optional post-release tags.
///
/// Examples:
/// - `"1.0"`
/// - `"1.0-a.0"`
/// - `"1.0-a.0,b.1"`
/// - `"1.0+c.0"`
/// - `"1.0+c.0,d.1"`
/// - `"1.0-a.0+c.0"`
/// - `"1.0-a.0,b.1+c.0,d.1"`
pub(crate) fn version_str<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "version_str",
        recognize(pair(
            separated_list1(char('.'), digit1),
            pair(
                opt(preceded(char('-'), ptagset)),
                opt(preceded(char('+'), ptagset)),
            ),
        )),
    )(input)
}
