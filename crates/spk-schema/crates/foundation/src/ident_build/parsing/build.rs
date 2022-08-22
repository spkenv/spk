// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use nom::{
    branch::alt,
    bytes::complete::take_while_m_n,
    combinator::map_res,
    error::{ContextError, FromExternalError, ParseError},
    IResult,
};
use nom_supreme::tag::{complete::tag, TagError};

use crate::ident_build::Build;

/// Parse a base32 build.
///
/// Example: `"CU7ZWOIF"`
///
/// The input must be of length [`crate::ident_build::DIGEST_SIZE`].
pub(crate) fn base32_build<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    take_while_m_n(
        crate::option_map::DIGEST_SIZE,
        crate::option_map::DIGEST_SIZE,
        is_base32_digit,
    )(input)
}

/// Parse a build into a [`Build`].
///
/// See [build_str] for examples of valid build strings.
pub fn build<'a, E>(input: &'a str) -> IResult<&'a str, Build, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::ident_build::error::Error>
        + TagError<&'a str, &'static str>,
{
    map_res(build_str, Build::from_str)(input)
}

/// Parse a build.
///
/// Examples:
/// - `"src"`
/// - `"embedded"`
/// - `"CU7ZWOIF"`
pub fn build_str<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + TagError<&'a str, &'static str>,
{
    alt((
        tag(crate::ident_build::SRC),
        tag(crate::ident_build::EMBEDDED),
        base32_build,
    ))(input)
}

#[inline]
pub(crate) fn is_base32_digit(c: char) -> bool {
    ('A'..='Z').contains(&c) || ('2'..='7').contains(&c)
}
