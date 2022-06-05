// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    character::complete::char,
    combinator::{all_consuming, map, opt},
    error::{context, ContextError, FromExternalError, ParseError},
    sequence::preceded,
    IResult,
};

use crate::api::{Build, Ident, Version};

use super::{
    name::package_name,
    parse_until, repo_name_in_ident,
    version::{version, version_str},
    version_and_optional_build,
};

/// Parse a package identity into an [`Ident`].
///
/// `known_repositories` is used to disambiguate input that
/// can be parsed in multiple ways. The first element of the
/// identity is more likely to be interpreted as a repository
/// name if it is a known repository name.
///
/// Examples:
/// - `"pkg-name"`
/// - `"pkg-name/1.0"`
/// - `"pkg-name/1.0/CU7ZWOIF"`
/// - `"repo-name/pkg-name"`
/// - `"repo-name/pkg-name/1.0"`
/// - `"repo-name/pkg-name/1.0/CU7ZWOIF"`
pub(crate) fn ident<'a, 'b, E>(
    known_repositories: &'a HashSet<&str>,
    input: &'b str,
) -> IResult<&'b str, Ident, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>,
{
    let (input, repository_name) = opt(repo_name_in_ident(
        known_repositories,
        package_ident,
        version_str,
        version_and_build,
    ))(input)?;
    let (input, mut ident) = package_ident(input)?;
    ident.repository_name = repository_name;
    let (input, version_and_build) = all_consuming(context(
        "ident version",
        opt(preceded(char('/'), version_and_build)),
    ))(input)?;
    match version_and_build {
        Some(v_and_b) => {
            ident.version = v_and_b.0;
            ident.build = v_and_b.1;
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
fn package_ident<'a, E>(input: &'a str) -> IResult<&'a str, Ident, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    parse_until("/", map(package_name, |name| Ident::new(name.to_owned())))(input)
}

/// Parse a version and optional build in the context of an identity string.
///
/// This function parses into [`Version`] and [`Build`] instances.
///
/// See [parse_version] for details on valid inputs.
fn version_and_build<'a, E>(input: &'a str) -> IResult<&'a str, (Version, Option<Build>), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    version_and_optional_build(context("parse_version", parse_until("/", version)))(input)
}
