// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{BTreeMap, HashSet};

use nom::{
    character::complete::{char, digit1, one_of},
    combinator::{map, map_res, opt, recognize},
    error::{context, ContextError, FromExternalError, ParseError},
    multi::{many1, separated_list1},
    sequence::{pair, preceded, separated_pair},
    IResult,
};

use crate::api::{InvalidVersionError, TagSet, Version};

use super::name::tag_name;

/// Parse a valid version pre- or post-tag.
///
/// See [ptag_str] for examples of valid tag strings.
pub(crate) fn ptag<'a, E>(input: &'a str) -> IResult<&'a str, (&'a str, u32), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    separated_pair(
        tag_name,
        char('.'),
        map_res(recognize(many1(one_of("0123456789"))), |n: &str| {
            n.parse::<u32>()
        }),
    )(input)
}

/// Parse a valid version pre- or post-tag.
///
/// A valid tag is comprised of a string and a number, separated
/// by a `.`.
///
/// Examples:
/// - `"r.0"`
/// - `"alpha1.400"`
pub(crate) fn ptag_str<'a, E>(input: &'a str) -> IResult<&'a str, (&'a str, &'a str), E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    separated_pair(tag_name, char('.'), digit1)(input)
}

/// Parse a valid pre- or post-tag set into a [`TagSet`].
///
/// See [ptagset_str] for examples of valid tag set strings.
pub(crate) fn ptagset<'a, E>(input: &'a str) -> IResult<&'a str, TagSet, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    map_res(separated_list1(char(','), ptag), |vec| {
        let mut tags = BTreeMap::new();
        for (name, num) in vec {
            if tags.insert(name.to_owned(), num).is_some() {
                return Err(InvalidVersionError::new_error(format!(
                    "duplicate tag: {}",
                    name
                )));
            }
        }
        Ok(TagSet { tags })
    })(input)
}

/// Parse a valid pre- or post-tag set.
///
/// A tag set is a comma-separated list of [`ptag`].
///
/// The string portion of the tag may not be repeated within a tag set.
///
/// Example: `"r.0,alpha1.400"`
pub(crate) fn ptagset_str<'a, E>(input: &'a str) -> IResult<&'a str, Vec<(&'a str, &'a str)>, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>,
{
    map_res(separated_list1(char(','), ptag_str), |tags| {
        let mut set = HashSet::with_capacity(tags.len());
        for (name, _) in &tags {
            if !set.insert(*name) {
                return Err(InvalidVersionError::new_error(format!(
                    "duplicate tag: {}",
                    name
                )));
            }
        }
        Ok(tags)
    })(input)
}

/// Parse a version string into a [`Version`].
///
/// See [version_str] for examples of valid version strings.
pub(crate) fn version<'a, E>(input: &'a str) -> IResult<&'a str, Version, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    context(
        "version",
        map(
            pair(
                separated_list1(
                    char('.'),
                    map_res(recognize(many1(one_of("0123456789"))), |n: &str| {
                        n.parse::<u32>()
                    }),
                ),
                pair(
                    context("optional pre-tag", opt(preceded(char('-'), ptagset))),
                    context("optional post-tag", opt(preceded(char('+'), ptagset))),
                ),
            ),
            |(parts, (pre, post))| Version {
                parts: parts.into(),
                pre: pre.unwrap_or_default(),
                post: post.unwrap_or_default(),
            },
        ),
    )(input)
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
/// - `"1.0+c.0-c.0"`
/// - `"1.0-a.0,b.1+c.0,d.1"`
pub(crate) fn version_str<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>,
{
    context(
        "version_str",
        recognize(pair(
            separated_list1(char('.'), digit1),
            pair(
                context(
                    "optional pre-tag",
                    opt(preceded(char('-'), recognize(ptagset_str))),
                ),
                context(
                    "optional post-tag",
                    opt(preceded(char('+'), recognize(ptagset_str))),
                ),
            ),
        )),
    )(input)
}
