// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::char,
    combinator::{eof, map, map_res, opt, peek},
    error::{context, VerboseError},
    sequence::{preceded, terminated},
    IResult,
};

use crate::api::{parse_version, Build, Ident, Version};

use super::{
    name::package_name, repo_name_in_ident, version::version_str, version_and_optional_build,
};

pub(crate) fn ident<'a, 'b>(
    known_repositories: &'a HashSet<&str>,
    input: &'b str,
) -> IResult<&'b str, Ident, VerboseError<&'b str>> {
    let (input, repository_name) = opt(repo_name_in_ident(
        known_repositories,
        package_ident,
        version_str,
        version_and_build,
    ))(input)?;
    let (input, mut ident) = package_ident(input)?;
    ident.repository_name = repository_name;
    let (input, version_and_build) = opt(preceded(char('/'), version_and_build))(input)?;
    eof(input)?;
    match version_and_build {
        Some(v_and_b) => {
            ident.version = v_and_b.0;
            ident.build = v_and_b.1;
            Ok(("", ident))
        }
        None => Ok(("", ident)),
    }
}

fn package_ident(input: &str) -> IResult<&str, Ident, VerboseError<&str>> {
    terminated(
        map(package_name, |name| Ident::new(name.to_owned())),
        peek(alt((tag("/"), eof))),
    )(input)
}

fn version_and_build(input: &str) -> IResult<&str, (Version, Option<Build>), VerboseError<&str>> {
    version_and_optional_build(context(
        "parse_version",
        map_res(version_str, parse_version),
    ))(input)
}
