// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while_m_n},
    combinator::map_res,
    error::{ContextError, FromExternalError, ParseError},
    IResult,
};

use crate::api::Build;

pub(crate) fn base32_build<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    take_while_m_n(
        crate::api::DIGEST_SIZE,
        crate::api::DIGEST_SIZE,
        is_base32_digit,
    )(input)
}

pub(crate) fn build<'a, E>(input: &'a str) -> IResult<&'a str, Build, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>,
{
    map_res(build_str, Build::from_str)(input)
}

pub(crate) fn build_str<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    alt((
        tag(crate::api::SRC),
        tag(crate::api::EMBEDDED),
        base32_build,
    ))(input)
}

#[inline]
pub(crate) fn is_base32_digit(c: char) -> bool {
    ('A'..='Z').contains(&c) || ('2'..='7').contains(&c)
}
