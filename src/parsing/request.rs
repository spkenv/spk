// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag},
    character::complete::char,
    combinator::{eof, map, map_parser, opt, peek},
    error::{context, ContextError, FromExternalError, ParseError},
    sequence::{pair, preceded, terminated},
    IResult,
};

use crate::api::{Build, Component, PkgName, RangeIdent, VersionFilter};

use super::{
    component::components, name::package_name, repo_name_in_ident, version_and_optional_build,
    version_range::version_range,
};

/// Parse a package name in the context of a range identity.
///
/// The package name must either be followed by a `/` or the end of input.
///
/// Examples:
/// - `"package-name"`
/// - `"package-name/"`
/// - `"package-name:comp"`
/// - `"package-name:{comp1,comp2}/"`
fn range_ident_pkg_name<'a, E>(
    input: &'a str,
) -> IResult<&'a str, (&PkgName, HashSet<Component>), E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    terminated(
        pair(
            package_name,
            map(opt(preceded(char(':'), components)), |opt_components| {
                opt_components.unwrap_or_default()
            }),
        ),
        peek(alt((tag("/"), eof))),
    )(input)
}

/// Parse a version filter in the context of a range identity.
///
/// Normally an empty string is a valid version filter, but in this
/// context it is not.
///
///   Legal: `"pkg-name/1.0/src"`
///
/// Illegal: `"pkg-name//src"`
///
/// See [version_range] for more details about parsing a version filter.
fn range_ident_version_filter<'a, E>(input: &'a str) -> IResult<&'a str, VersionFilter, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    context(
        "range_ident_version_filter",
        map(map_parser(is_not("/"), version_range(true, true)), |v| {
            VersionFilter {
                rules: v.into_iter().collect(),
            }
        }),
    )(input)
}

/// Parse a package range identity into a [`RangeIdent`].
///
/// `known_repositories` is used to disambiguate input that
/// can be parsed in multiple ways. The first element of the
/// identity is more likely to be interpreted as a repository
/// name if it is a known repository name.
///
/// Like [`super::ident`], but the package name portion may
/// name components, and the version portion is a version
/// filter expression.
pub(crate) fn range_ident<'a, 'b, E>(
    known_repositories: &'a HashSet<&str>,
    input: &'b str,
) -> IResult<&'b str, RangeIdent, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>,
{
    let (input, repository_name) = opt(repo_name_in_ident(
        known_repositories,
        range_ident_pkg_name,
        range_ident_version_filter,
        version_filter_and_build,
    ))(input)?;
    let (input, (name, components)) = range_ident_pkg_name(input)?;
    let (input, (version, build)) = map(
        opt(preceded(char('/'), version_filter_and_build)),
        |v_and_b| v_and_b.unwrap_or_default(),
    )(input)?;
    eof(input)?;
    Ok((
        "",
        RangeIdent {
            repository_name,
            name: name.to_owned(),
            components,
            version,
            build,
        },
    ))
}

/// Parse a version filter and optional build in the context of a
/// range identity.
///
/// This function parses into [`VersionFilter`] and [`Build`] instances.
///
/// See [range_ident_version_filter] for details on valid inputs.
pub(crate) fn version_filter_and_build<'a, E>(
    input: &'a str,
) -> IResult<&'a str, (VersionFilter, Option<Build>), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>,
{
    version_and_optional_build(context("parse_version_filter", range_ident_version_filter))(input)
}
