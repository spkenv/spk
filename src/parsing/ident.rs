// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    character::complete::char,
    combinator::{all_consuming, cut, map, opt},
    error::{ContextError, FromExternalError, ParseError},
    sequence::{pair, preceded},
    IResult,
};
use nom_supreme::tag::TagError;

use crate::api::{AnyId, Build, BuildId, RepositoryName, Version, VersionId};

use super::{
    build::{build, build_str},
    name::package_name,
    repo_name_in_ident,
    version::{version, version_str},
    version_and_optional_build,
};

/// Parse a package identity into an [`AnyId`].
///
/// Examples:
/// - `"pkg-name"`
/// - `"pkg-name/1.0"`
/// - `"pkg-name/1.0/CU7ZWOIF"`
pub(crate) fn any_id<'b, E>(input: &'b str) -> IResult<&'b str, AnyId, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    let (input, VersionId { name, .. }) = package_ident(input)?;
    let (input, version_and_build) = all_consuming(opt(preceded(
        char('/'),
        version_and_optional_build(version, build),
    )))(input)?;
    let id = match version_and_build {
        None => AnyId::Version(VersionId {
            name,
            version: Version::default(),
        }),
        Some((version, None)) => AnyId::Version(VersionId { name, version }),
        Some((version, Some(build))) => AnyId::Build(BuildId {
            name,
            version,
            build,
        }),
    };
    Ok((input, id))
}

/// Parse a package identity into a [`BuildId`].
///
/// Examples:
/// - `"pkg-name/1.0/CU7ZWOIF"`
/// - `"pkg-name/1.0/embedded"`
/// - `"pkg-name/1.0/src"`
pub(crate) fn build_id<'b, E>(input: &'b str) -> IResult<&'b str, BuildId, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    let (input, VersionId { name, .. }) = package_ident(input)?;
    let (input, (version, build)) = all_consuming(preceded(char('/'), version_and_build))(input)?;
    Ok((
        input,
        BuildId {
            name,
            version,
            build,
        },
    ))
}

/// Parse a package identity into a [`VersionId`].
///
/// Examples:
/// - `"pkg-name"`
/// - `"pkg-name/1.0"`
/// - `"pkg-name/1.0.0-beta.1"`
pub(crate) fn version_id<'b, E>(input: &'b str) -> IResult<&'b str, VersionId, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    let (input, mut ident) = package_ident(input)?;
    let (input, version) = all_consuming(opt(preceded(char('/'), version)))(input)?;
    if let Some(v) = version {
        ident.version = v;
    }
    Ok((input, ident))
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
        version_and_optional_build(version, build),
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

/// Parse a package name in the context of an identity string into a [`VersionId`].
///
/// The package name must either be followed by a `/` or the end of input.
///
/// Examples:
/// - `"package-name"`
/// - `"package-name/"`
fn package_ident<'a, E>(input: &'a str) -> IResult<&'a str, VersionId, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(package_name, |name| VersionId::from(name.to_owned()))(input)
}

/// Parse a version and optional build in the context of an identity string.
///
/// This function parses into [`Version`] and [`Build`] instances.
///
/// See [`crate::api::parse_version`] for details on valid inputs.
fn version_and_build<'a, E>(input: &'a str) -> IResult<&'a str, (Version, Build), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    pair(version, preceded(char('/'), cut(build)))(input)
}
