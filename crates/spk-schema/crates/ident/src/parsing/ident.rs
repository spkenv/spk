// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use nom::character::complete::char;
use nom::combinator::{map, opt};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::sequence::preceded;
use nom::IResult;
use nom_supreme::tag::TagError;
use spk_schema_foundation::ident_ops::parsing::version_and_build;
use spk_schema_foundation::name::parsing::package_name;

use crate::{AnyIdent, VersionIdent};

/// Parse a package identity into an [`AnyIdent`].
///
/// Examples:
/// - `"pkg-name"`
/// - `"pkg-name/1.0"`
/// - `"pkg-name/1.0/CU7ZWOIF"`
pub fn ident<'b, E>(input: &'b str) -> IResult<&'b str, AnyIdent, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, spk_schema_foundation::ident_build::Error>
        + FromExternalError<&'b str, spk_schema_foundation::version::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    let (input, ident) = package_ident(input)?;
    let mut ident = ident.into_any(None);
    let (input, version_and_build) = opt(preceded(char('/'), version_and_build))(input)?;
    match version_and_build {
        Some(v_and_b) => {
            ident.base.target = v_and_b.0;
            ident.target = v_and_b.1;
            Ok((input, ident))
        }
        None => Ok((input, ident)),
    }
}

/// Parse a package name in the context of an identity string into an [`Ident`].
///
/// The package name must either be followed by a `/` or the end of input.
///
/// Examples:
/// - `"package-name"`
/// - `"package-name/"`
fn package_ident<'a, E>(input: &'a str) -> IResult<&'a str, VersionIdent, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(package_name, |name| VersionIdent::new_zero(name.to_owned()))(input)
}
