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
    combinator::opt,
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
