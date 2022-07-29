// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    character::complete::char,
    combinator::{all_consuming, map, opt},
    error::{ContextError, FromExternalError, ParseError},
    sequence::preceded,
    IResult,
};
use nom_supreme::tag::TagError;

use crate::api::{Build, Ident, RepositoryName, Version};

use super::{
    build::{build, build_str},
    name::package_name,
    repo_name_in_ident,
    version::{version, version_str},
    version_and_optional_build,
};

/// Parse a package identity into an [`Ident`].
///
/// Examples:
/// - `"pkg-name"`
/// - `"pkg-name/1.0"`
/// - `"pkg-name/1.0/CU7ZWOIF"`
pub(crate) fn ident<'b, E>(input: &'b str) -> IResult<&'b str, Ident, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    let (input, mut ident) = package_ident(input)?;
    let (input, version_and_build) =
        all_consuming(opt(preceded(char('/'), version_and_build)))(input)?;
    match version_and_build {
        Some(v_and_b) => {
            ident.version = v_and_b.0;
            ident.build = v_and_b.1;
            Ok((input, ident))
        }
        None => Ok((input, ident)),
    }
}

#[derive(Debug)]
pub struct IdentParts<'s> {
    pub repository_name: Option<&'s str>,
    pub pkg_name: &'s str,
    pub version_str: Option<&'s str>,
    pub build_str: Option<&'s str>,
}

/// Parse a package identity into parts.
///
/// Returns an [`IdentParts`] making it possible to identify which parts were
/// specified.
pub fn ident_parts<'a, 'b, E>(
    known_repositories: &'a HashSet<&str>,
    input: &'b str,
) -> IResult<&'b str, IdentParts<'b>, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    let (input, repository_name) = opt(repo_name_in_ident(
        known_repositories,
        package_ident,
        version_str,
        version_and_build,
    ))(input)?;
    let (input, pkg_name) = package_name(input)?;
    let (input, version_and_build) = all_consuming(opt(preceded(
        char('/'),
        version_and_optional_build(version_str, build_str),
    )))(input)?;
    match version_and_build {
        Some(v_and_b) => Ok((
            input,
            IdentParts {
                repository_name: repository_name.map(RepositoryName::as_str),
                pkg_name: pkg_name.as_str(),
                version_str: Some(v_and_b.0),
                build_str: v_and_b.1,
            },
        )),
        None => Ok((
            input,
            IdentParts {
                repository_name: repository_name.map(RepositoryName::as_str),
                pkg_name: pkg_name.as_str(),
                version_str: None,
                build_str: None,
            },
        )),
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
    map(package_name, |name| Ident::new(name.to_owned()))(input)
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
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    version_and_optional_build(version, build)(input)
}
