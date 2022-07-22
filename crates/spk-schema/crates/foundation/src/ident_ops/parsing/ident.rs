// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    character::complete::char,
    combinator::{opt, recognize},
    error::{ContextError, FromExternalError, ParseError},
    sequence::preceded,
    IResult,
};
use nom_supreme::tag::TagError;

use crate::{
    ident_build::parsing::build,
    name::{parsing::package_name, RepositoryName},
    version::parsing::version_str,
};

use super::{repo_name_in_ident, version_and_build, version_and_optional_build};

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
) -> impl FnMut(&'b str) -> IResult<&'b str, IdentParts<'b>, E> + 'a
where
    E: ParseError<&'b str>
        + ContextError<&'b str>
        + FromExternalError<&'b str, crate::ident_build::Error>
        + FromExternalError<&'b str, crate::version::Error>
        + FromExternalError<&'b str, std::num::ParseIntError>
        + TagError<&'b str, &'static str>,
{
    move |input: &str| {
        let (input, repository_name) = opt(repo_name_in_ident(
            known_repositories,
            package_name,
            version_str,
            version_and_build,
        ))(input)?;
        let (input, pkg_name) = package_name(input)?;
        let (input, version_and_build) = opt(preceded(
            char('/'),
            version_and_optional_build(version_str, recognize(build)),
        ))(input)?;
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
}
