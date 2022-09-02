// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use nom::{
    bytes::complete::take_while1,
    character::complete::char,
    combinator::{map_res, rest},
    error::{ContextError, FromExternalError, ParseError},
    sequence::separated_pair,
    IResult,
};
use spk_schema::TestStage;

/// Parse a package filename with a stage specifier.
///
/// Examples:
/// - "package.spk.yaml@build"
pub fn stage_specifier<'a, E>(input: &'a str) -> IResult<&'a str, (&'a str, TestStage), E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + FromExternalError<&'a str, spk_schema::Error>,
{
    separated_pair(
        take_while1(|c| c != '@'),
        char('@'),
        map_res(rest, TestStage::from_str),
    )(input)
}
