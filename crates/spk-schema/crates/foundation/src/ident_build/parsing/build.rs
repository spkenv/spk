// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::convert::TryInto;

use nom::branch::alt;
use nom::bytes::complete::take_while_m_n;
use nom::combinator::{cut, map, map_res, opt, verify};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::sequence::{delimited, preceded};
use nom::IResult;
use nom_supreme::tag::complete::tag;
use nom_supreme::tag::TagError;

use crate::ident_build::build::{EmbeddedSource, EmbeddedSourcePackage};
use crate::ident_build::{Build, InvalidBuildError};
use crate::ident_ops::parsing::ident_parts_with_components;

/// Parse a base32 build.
///
/// Example: `"CU7ZWOIF"`
///
/// The input must be of length [`crate::ident_build::DIGEST_SIZE`].
pub(crate) fn base32_build<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    verify(
        take_while_m_n(
            crate::option_map::DIGEST_SIZE,
            crate::option_map::DIGEST_SIZE,
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
pub fn build<'a, E>(input: &'a str) -> IResult<&'a str, Build, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::version::Error>
        + FromExternalError<&'a str, crate::ident_build::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    alt((
        map(tag(crate::ident_build::SRC), |_| Build::Source),
        map(
            preceded(tag(crate::ident_build::EMBEDDED), cut(embedded_source)),
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
        + FromExternalError<&'b str, crate::version::Error>
        + FromExternalError<&'b str, crate::ident_build::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    map(opt(embedded_source_package), |ident_and_components| {
        ident_and_components.unwrap_or(EmbeddedSource::Unknown)
    })(input)
}

pub fn embedded_source_package<'b, E>(input: &'b str) -> IResult<&'b str, EmbeddedSource, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::ident_build::Error>
        + FromExternalError<&'b str, crate::version::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    map(
        delimited(tag("["), cut(ident_parts_with_components), cut(tag("]"))),
        |(ident, components)| {
            EmbeddedSource::Package(Box::new(EmbeddedSourcePackage {
                ident: ident.to_owned(),
                components,
            }))
        },
    )(input)
}

#[inline]
pub(crate) fn is_base32_digit(c: char) -> bool {
    c.is_ascii_uppercase() || ('2'..='7').contains(&c)
}
