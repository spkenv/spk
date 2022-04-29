// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    combinator::{eof, fail, map, opt, peek},
    error::{context, VerboseError},
    multi::separated_list1,
    sequence::{pair, preceded, terminated},
    IResult,
};

use crate::api::{Build, Component, PkgName, RangeIdent, RepositoryName, VersionFilter};

use super::{
    build::build,
    component::components,
    name::{is_legal_package_name_chr, known_repository_name, package_name, repository_name},
    version_range::version_range,
};

fn package_name_and_not_version_filter(
    input: &str,
) -> IResult<&str, (&PkgName, HashSet<Component>), VerboseError<&str>> {
    let (tail, ident) = range_ident_pkg_name(input)?;
    // To disambiguate cases like:
    //    111/222
    // If "222" is a valid version string and is the end of input,
    // return an Error here so that "111" will be treated as the
    // package name instead of as a repository name.
    if terminated(range_ident_version_filter, eof)(input).is_ok() {
        return fail("could be version filter");
    }
    // To disambiguate cases like:
    //    222/333/44444444
    // If "333" is a valid version string and "44444444" is a
    // valid build string and is the end of input, return an Error
    // here so that "222" will be treated as the package name
    // instead of as a repository name.
    let prefixed = format!("/{}", input);
    if let Ok((_, (_version, Some(_build)))) =
        terminated(version_filter_and_build, eof)(prefixed.as_str())
    {
        return fail("could be a build");
    }
    Ok((tail, ident))
}

fn range_ident_pkg_name(
    input: &str,
) -> IResult<&str, (&PkgName, HashSet<Component>), VerboseError<&str>> {
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

fn range_ident_version_filter(input: &str) -> IResult<&str, VersionFilter, VerboseError<&str>> {
    context(
        "range_ident_version_filter",
        map(
            separated_list1(tag(crate::api::VERSION_RANGE_SEP), version_range),
            |v| VersionFilter {
                rules: v.into_iter().collect(),
            },
        ),
    )(input)
}

pub(crate) fn range_ident<'a, 'b>(
    known_repositories: &'a HashSet<&str>,
    input: &'b str,
) -> IResult<&'b str, RangeIdent, VerboseError<&'b str>> {
    let (input, repository_name) = opt(repo_name_in_range_ident(known_repositories))(input)?;
    let (input, (name, components)) = range_ident_pkg_name(input)?;
    let (input, (version, build)) = map(opt(version_filter_and_build), |v_and_b| {
        v_and_b.unwrap_or_default()
    })(input)?;
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

fn repo_name_in_range_ident<'a>(
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
                peek(package_name_and_not_version_filter),
            ),
        ))(input)
    }
}

fn version_filter_and_build(
    input: &str,
) -> IResult<&str, (VersionFilter, Option<Build>), VerboseError<&str>> {
    pair(
        preceded(
            char('/'),
            context("parse_version_filter", range_ident_version_filter),
        ),
        opt(context("parse_build", build)),
    )(input)
}
