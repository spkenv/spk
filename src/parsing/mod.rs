// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
mod component;
mod ident;
mod name;
mod request;
mod version;
mod version_range;

use nom::{
    character::complete::char,
    combinator::{eof, fail, opt},
    error::{context, VerboseError},
    sequence::{pair, preceded},
    IResult, Parser,
};

pub(crate) use ident::ident;
pub(crate) use request::{range_ident, version_filter_and_build};

use crate::api::Build;

use self::build::build;

#[cfg(test)]
#[path = "./parsing_test.rs"]
mod parsing_test;

/// Parse a package name.
///
/// Succeeds if the input can be parsed as a package name,
/// and cannot be parsed as version or version and build.
///
/// This function is generic over the type of package-like and
/// version-like expression that is expected.
pub(crate) fn package_name_and_not_version<'i, I, V1, V2, B, F1, F2, F3>(
    mut ident_parser: F1,
    mut version_parser: F2,
    mut version_and_build_parser: F3,
) -> impl FnMut(&'i str) -> IResult<&'i str, I, VerboseError<&'i str>>
where
    F1: Parser<&'i str, I, VerboseError<&'i str>>,
    F2: Parser<&'i str, V1, VerboseError<&'i str>>,
    F3: Parser<&'i str, (V2, Option<B>), VerboseError<&'i str>>,
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

/// Expect a version-like expression and optional build.
///
/// This function is generic over the type of version-like expression
/// that is expected.
pub(crate) fn version_and_optional_build<'i, V, F>(
    version_parser: F,
) -> impl FnMut(&'i str) -> IResult<&'i str, (V, Option<Build>), VerboseError<&'i str>>
where
    F: Parser<&'i str, V, VerboseError<&'i str>>,
{
    pair(
        version_parser,
        opt(preceded(char('/'), context("parse_build", build))),
    )
}
