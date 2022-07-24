// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryInto;

use nom::{
    branch::alt,
    bytes::complete::take_while_m_n,
    combinator::{cut, map, map_res, opt, verify},
    error::{ContextError, FromExternalError, ParseError},
    sequence::{delimited, preceded},
    IResult,
};
use nom_supreme::tag::{complete::tag, TagError};

use crate::api::{Build, EmbeddedSource, InvalidBuildError};

use super::ident::ident_with_components;

/// Parse a base32 build.
///
/// Example: `"CU7ZWOIF"`
///
/// The input must be of length [`crate::api::DIGEST_SIZE`].
pub(crate) fn base32_build<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    verify(
        take_while_m_n(
            crate::api::DIGEST_SIZE,
            crate::api::DIGEST_SIZE,
            is_base32_digit,
        ),
        |bytes: &str| data_encoding::BASE32.decode(bytes.as_bytes()).is_ok(),
    )(input)
}

/// Parse a build into a [`Build`].
///
/// Examples:
/// - `"src"`
/// - `"embedded[pkg/1.0/CU7ZWOIF]"`
/// - `"embedded"` (legacy format)
/// - `"CU7ZWOIF"`
pub(crate) fn build<'a, E>(input: &'a str) -> IResult<&'a str, Build, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    alt((
        map(tag(crate::api::SRC), |_| Build::Source),
        map(
            preceded(tag(crate::api::EMBEDDED), cut(embedded_source)),
            Build::Embedded,
        ),
        map_res(base32_build, |digest| {
            digest
                .chars()
                .collect::<Vec<_>>()
                .try_into()
                .map_err(|err| {
                    InvalidBuildError::new_error(format!(
                        "Invalid build digest '{digest}': {err:?}"
                    ))
                })
                .map(Build::Digest)
        }),
    ))(input)
}

fn embedded_source<'b, E>(input: &'b str) -> IResult<&'b str, EmbeddedSource, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    map(opt(embedded_source_package), |ident_and_components| {
        ident_and_components.unwrap_or(EmbeddedSource::Unknown)
    })(input)
}

pub(crate) fn embedded_source_package<'b, E>(input: &'b str) -> IResult<&'b str, EmbeddedSource, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    map(
        delimited(tag("["), cut(ident_with_components), cut(tag("]"))),
        |(ident, components)| EmbeddedSource::Package {
            ident: Box::new(ident),
            components,
        },
    )(input)
}

#[inline]
pub(crate) fn is_base32_digit(c: char) -> bool {
    ('A'..='Z').contains(&c) || ('2'..='7').contains(&c)
}
