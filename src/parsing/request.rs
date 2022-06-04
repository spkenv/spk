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
