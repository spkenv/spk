// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::character::complete::char;
use nom::combinator::{cut, map, opt};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::sequence::preceded;
use nom::IResult;
use nom_supreme::tag::TagError;
use spk_schema_foundation::ident_build::parsing::build;
use spk_schema_foundation::ident_build::Build;
use spk_schema_foundation::ident_ops::parsing::{
    range_ident_pkg_name,
    repo_name_in_ident,
    version_and_optional_build,
};
use spk_schema_foundation::version_range::parsing::version_range;
use spk_schema_foundation::version_range::VersionFilter;

use crate::RangeIdent;

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
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, spk_schema_foundation::version::Error>
        + FromExternalError<&'a str, spk_schema_foundation::version_range::Error>
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
) -> impl FnMut(&'b str) -> IResult<&'b str, RangeIdent, E> + 'a
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::error::Error>
        + FromExternalError<&'b str, spk_schema_foundation::ident_build::Error>
        + FromExternalError<&'b str, spk_schema_foundation::version::Error>
        + FromExternalError<&'b str, spk_schema_foundation::version_range::Error>
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
        + FromExternalError<&'a str, crate::error::Error>
        + FromExternalError<&'a str, spk_schema_foundation::ident_build::Error>
        + FromExternalError<&'a str, spk_schema_foundation::version::Error>
        + FromExternalError<&'a str, spk_schema_foundation::version_range::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    version_and_optional_build(range_ident_version_filter, build)(input)
}
