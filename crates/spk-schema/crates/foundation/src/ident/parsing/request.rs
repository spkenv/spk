// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;

use nom::IResult;
use nom::character::complete::char;
use nom::combinator::{all_consuming, cut, map, opt};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::multi::separated_list1;
use nom::sequence::preceded;
use nom_supreme::tag::TagError;

use crate::ident::RangeIdent;
use crate::ident_build::Build;
use crate::ident_build::parsing::build;
use crate::ident_ops::parsing::{
    range_ident_pkg_name, repo_name_in_ident, version_and_optional_build,
};
use crate::version_range::VersionFilter;
use crate::version_range::parsing::version_range;

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
pub fn range_ident_version_filter<'a, E>(input: &'a str) -> IResult<&'a str, VersionFilter, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::ident::error::Error>
        + FromExternalError<&'a str, crate::version::Error>
        + FromExternalError<&'a str, crate::version_range::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    map(version_range, VersionFilter::new)(input)
}

/// Parse a package range identity into a [`RangeIdent`].
///
/// `known_repositories` is used to disambiguate input that
/// can be parsed in multiple ways. The first element of the
/// identity is more likely to be interpreted as a repository
/// name if it is a known repository name.
///
/// Like [`super::ident()`], but the package name portion may
/// name components, and the version portion is a version
/// filter expression.
pub fn range_ident<'a, 'b, E>(
    known_repositories: &'a HashSet<&str>,
) -> impl FnMut(&'b str) -> IResult<&'b str, RangeIdent, E>
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::ident::error::Error>
        + FromExternalError<&'b str, crate::ident_build::Error>
        + FromExternalError<&'b str, crate::version::Error>
        + FromExternalError<&'b str, crate::version_range::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    move |input: &str| {
        let (input, repository_name) = opt(repo_name_in_ident(
            known_repositories,
            range_ident_pkg_name,
            range_ident_version_filter,
            version_filter_and_build,
        ))(input)?;
        let (input, (name, components)) = range_ident_pkg_name(input)?;
        let (input, (version, build)) = map(
            opt(preceded(char('/'), cut(version_filter_and_build))),
            |v_and_b| v_and_b.unwrap_or_default(),
        )(input)?;
        Ok((
            input,
            RangeIdent {
                repository_name: repository_name.map(ToOwned::to_owned),
                name: name.to_owned(),
                components,
                version,
                build,
            },
        ))
    }
}

/// Parse a version filter and optional build in the context of a
/// range identity.
///
/// This function parses into [`VersionFilter`] and [`Build`] instances.
///
/// See [`range_ident_version_filter`] for details on valid inputs.
pub fn version_filter_and_build<'a, E>(
    input: &'a str,
) -> IResult<&'a str, (VersionFilter, Option<Build>), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::ident::error::Error>
        + FromExternalError<&'a str, crate::ident_build::Error>
        + FromExternalError<&'a str, crate::version::Error>
        + FromExternalError<&'a str, crate::version_range::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    version_and_optional_build(range_ident_version_filter, build)(input)
}

/// Parse a comma separated list of range idents (requests)
///
/// This function parses into a list of [`RangeIdent`] instances.
///
/// `known_repositories` is used to disambiguate input that
/// can be parsed in multiple ways. The first element of the
/// identity is more likely to be interpreted as a repository
/// name if it is a known repository name.
///
/// Examples:
/// - python,maya,openimageio,zlib
/// - python,maya/2022.3,openimageio,zlib/1.2.11
/// - python,local/maya/2022.3,openimageio,zlib/1.2.11/ABCDEF
///
/// See [`range_ident`] for details on parsing a range ident.
pub fn range_ident_comma_separated_list(
    known_repositories: &HashSet<&str>,
    input: &str,
) -> Result<Vec<RangeIdent>, crate::ident::Error> {
    let parsed_list = all_consuming(separated_list1(
        char(','),
        range_ident::<nom_supreme::error::ErrorTree<_>>(known_repositories),
    ))(input);

    parsed_list.map(|(_, l)| l).map_err(|err| match err {
        nom::Err::Error(e) | nom::Err::Failure(e) => crate::ident::Error::String(e.to_string()),
        nom::Err::Incomplete(_) => unreachable!(),
    })
}
