// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::branch::alt;
use nom::bytes::complete::take_while1;
use nom::character::complete::char;
use nom::combinator::{cut, eof, fail, opt, peek};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::sequence::{pair, preceded, terminated};
use nom::{IResult, Parser};
use nom_supreme::tag::complete::tag;
use nom_supreme::tag::TagError;
use once_cell::sync::Lazy;

use crate::ident_build::parsing::build;
use crate::ident_build::Build;
use crate::name::parsing::{is_legal_package_name_chr, known_repository_name, repository_name};
use crate::name::RepositoryName;
use crate::version::parsing::version;
use crate::version::Version;

mod ident;
mod request;

pub use ident::{ident_parts, ident_parts_with_components, IdentParts, IdentPartsBuf};
pub use request::range_ident_pkg_name;

pub static KNOWN_REPOSITORY_NAMES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    let mut known_repositories = HashSet::from(["local"]);
    if let Ok(config) = spfs::get_config() {
        for name in config.list_remote_names() {
            // Leak these Strings; they require 'static lifetime.
            let name = Box::leak(Box::new(name));
            known_repositories.insert(name);
        }
    }
    known_repositories
});

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
    E: ParseError<&'i str> + ContextError<&'i str> + TagError<&'i str, &'static str>,
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
            .and_then(|(input, _)| alt((tag("]"), eof::<&str, _>))(input));
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
            .and_then(|(input, v_and_b)| {
                alt((tag("]"), eof))(input).map(|(input, _)| (input, v_and_b))
            });
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
pub fn repo_name_in_ident<'a, 'i, I, V1, V2, B, F1, F2, F3, E>(
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
    E: ParseError<&'i str> + ContextError<&'i str> + TagError<&'i str, &'static str> + 'a,
{
    // To disambiguate cases like:
    //    local/222
    // If "local" is a known repository name and "222" is a valid
    // package name and the end of input, treat the first component
    // as a repository name instead of a package name.
    alt((
        terminated(
            terminated(known_repository_name(known_repositories), char('/')),
            peek(terminated(
                take_while1(is_legal_package_name_chr),
                alt((tag("]"), eof)),
            )),
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

/// Parse a version and optional build in the context of an identity string.
///
/// This function parses into [`Version`] and [`Build`] instances.
///
/// See [crate::version::parse_version] for details on valid inputs.
pub fn version_and_build<'a, E>(input: &'a str) -> IResult<&'a str, (Version, Option<Build>), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::ident_build::Error>
        + FromExternalError<&'a str, crate::version::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    version_and_optional_build(version, build)(input)
}

/// Parse a version and build in the context of an identity string.
///
/// This function parses into [`Version`] and [`Build`] instances.
///
/// See [crate::version::parse_version] for details on valid inputs.
pub fn version_and_required_build<'a, E>(input: &'a str) -> IResult<&'a str, (Version, Build), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::ident_build::Error>
        + FromExternalError<&'a str, crate::version::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    pair(version, preceded(char('/'), cut(build)))(input)
}

/// Expect a version-like expression and optional build.
///
/// This function is generic over the type of version-like expression
/// that is expected.
pub fn version_and_optional_build<'i, V, B, F1, F2, E>(
    version_parser: F1,
    build_parser: F2,
) -> impl FnMut(&'i str) -> IResult<&'i str, (V, Option<B>), E>
where
    F1: Parser<&'i str, V, E>,
    F2: Parser<&'i str, B, E>,
    E: ParseError<&'i str>
        + ContextError<&'i str>
        + FromExternalError<&'i str, std::num::ParseIntError>
        + TagError<&'i str, &'static str>,
{
    pair(version_parser, opt(preceded(char('/'), cut(build_parser))))
}
