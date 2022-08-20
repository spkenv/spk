// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod ident;
mod request;

#[cfg(test)]
#[path = "./parsing_test.rs"]
mod parsing_test;

use std::collections::HashSet;

use nom::{
    branch::alt,
    bytes::complete::take_while1,
    character::complete::char,
    combinator::{cut, eof, fail, opt, peek},
    error::{ContextError, FromExternalError, ParseError},
    sequence::{pair, preceded, terminated},
    IResult, Parser,
};
use nom_supreme::tag::TagError;
use spk_schema_foundation::name::{
    parsing::{is_legal_package_name_chr, known_repository_name, repository_name},
    RepositoryName,
};

pub use ident::{ident, ident_parts, IdentParts};
pub use request::{range_ident, range_ident_version_filter, version_filter_and_build};

/// Parse a package name.
///
/// Succeeds if the input can be parsed as a package name,
/// and cannot be parsed as version or version and build.
///
/// This function is generic over the type of package-like and
/// version-like expression that is expected.
fn package_name_and_not_version<'i, I, V1, V2, B, F1, F2, F3, E>(
    mut ident_parser: F1,
    mut version_parser: F2,
    mut version_and_build_parser: F3,
) -> impl FnMut(&'i str) -> IResult<&'i str, I, E>
where
    F1: Parser<&'i str, I, E>,
    F2: Parser<&'i str, V1, E>,
    F3: Parser<&'i str, (V2, Option<B>), E>,
    E: ParseError<&'i str> + ContextError<&'i str>,
{
    move |input: &str| {
        let (tail, ident) = ident_parser.parse(input)?;
        // To disambiguate cases like:
        //    111/222
        // If "222" is a valid version string and is the end of input,
        // return an Error here so that "111" will be treated as the
        // package name instead of as a repository name.
        let r = version_parser
            .parse(input)
            .and_then(|(input, _)| eof::<&str, _>(input));
        if r.is_ok() {
            return fail("could be version");
        }
        // To disambiguate cases like:
        //    222/333/44444444
        // If "333" is a valid version string and "44444444" is a
        // valid build string and is the end of input, return an Error
        // here so that "222" will be treated as the package name
        // instead of as a repository name.
        let r = version_and_build_parser
            .parse(input)
            .and_then(|(input, v_and_b)| eof(input).map(|(input, _)| (input, v_and_b)));
        if let Ok((_, (_version, Some(_build)))) = r {
            return fail("could be a build");
        }
        Ok((tail, ident))
    }
}

/// Expect a repository name in the context of an identity.
///
/// This parser expects that the repository name is followed by
/// a '/' within the input, and fails if the input is more likely
/// to be a package name, even if it might be a valid repository
/// name.
///
/// This function is generic over the type of package-like and
/// version-like expression that is expected.
pub(crate) fn repo_name_in_ident<'a, 'i, I, V1, V2, B, F1, F2, F3, E>(
    known_repositories: &'a HashSet<&'a str>,
    ident_parser: F1,
    version_parser: F2,
    version_and_build_parser: F3,
) -> impl FnMut(&'i str) -> IResult<&'i str, &'i RepositoryName, E> + 'a
where
    'i: 'a,
    I: 'a,
    B: 'a,
    V1: 'a,
    V2: 'a,
    F1: Parser<&'i str, I, E> + 'a,
    F2: Parser<&'i str, V1, E> + 'a,
    F3: Parser<&'i str, (V2, Option<B>), E> + 'a,
    E: ParseError<&'i str> + ContextError<&'i str> + 'a,
{
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
            peek(package_name_and_not_version(
                ident_parser,
                version_parser,
                version_and_build_parser,
            )),
        ),
    ))
}

/// Expect a version-like expression and optional build.
///
/// This function is generic over the type of version-like expression
/// that is expected.
pub(crate) fn version_and_optional_build<'i, V, B, F1, F2, E>(
    version_parser: F1,
    build_parser: F2,
) -> impl FnMut(&'i str) -> IResult<&'i str, (V, Option<B>), E>
where
    F1: Parser<&'i str, V, E>,
    F2: Parser<&'i str, B, E>,
    E: ParseError<&'i str>
        + ContextError<&'i str>
        + FromExternalError<&'i str, crate::error::Error>
        + TagError<&'i str, &'static str>,
{
    pair(version_parser, opt(preceded(char('/'), cut(build_parser))))
}
