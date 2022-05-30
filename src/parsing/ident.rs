// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::{eof, fail, map, map_res, opt, peek},
    error::{context, VerboseError},
    sequence::{preceded, terminated},
    IResult,
};

use crate::api::{parse_version, Build, Ident, RepositoryName, Version};

use super::{
    name::{is_legal_package_name_chr, known_repository_name, package_name, repository_name},
    version::version_str,
    version_and_optional_build,
};

pub(crate) fn ident<'a, 'b>(
    known_repositories: &'a HashSet<&str>,
    input: &'b str,
) -> IResult<&'b str, Ident, VerboseError<&'b str>> {
    let (input, repository_name) = opt(repo_name_in_ident(known_repositories))(input)?;
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

fn package_name_and_not_version(input: &str) -> IResult<&str, Ident, VerboseError<&str>> {
    let (tail, ident) = package_ident(input)?;
    // To disambiguate cases like:
    //    111/222
    // If "222" is a valid version string and is the end of input,
    // return an Error here so that "111" will be treated as the
    // package name instead of as a repository name.
    if terminated(version_str, eof)(input).is_ok() {
        return fail("could be version");
    }
    // To disambiguate cases like:
    //    222/333/44444444
    // If "333" is a valid version string and "44444444" is a
    // valid build string and is the end of input, return an Error
    // here so that "222" will be treated as the package name
    // instead of as a repository name.
    if let Ok((_, (_version, Some(_build)))) = terminated(version_and_build, eof)(input) {
        return fail("could be a build");
    }
    Ok((tail, ident))
}

fn repo_name_in_ident<'a>(
    known_repositories: &'a HashSet<&'a str>,
) -> impl Fn(&str) -> IResult<&str, RepositoryName, VerboseError<&str>> + 'a {
    move |input| {
        // To disambiguate cases like:
        //    local/222
        // If "local" is a known repository name and "222" is a valid
        // package name and the end of input, treat the first component
        // as a repository name instead of a package name.
        alt((
            terminated(
                terminated(known_repository_name(known_repositories), char('/')),
                peek(terminated(take_while1(is_legal_package_name_chr), eof)),
            ),
            terminated(
                terminated(repository_name, char('/')),
                // Reject treating the consumed component as a repository name if the following
                // components are more likely to mean the consumed component was actually a
                // package name. This puts more emphasis on interpreting input the same as before
                // repository names were added.
                peek(package_name_and_not_version),
            ),
        ))(input)
    }
}

fn version_and_build(input: &str) -> IResult<&str, (Version, Option<Build>), VerboseError<&str>> {
    version_and_optional_build(context(
        "parse_version",
        map_res(version_str, parse_version),
    ))(input)
}
